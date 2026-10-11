from __future__ import annotations

import json
import hashlib
import os
import posixpath
import re
import stat
import subprocess
import sys
import unicodedata
from pathlib import Path, PurePosixPath


SOURCE_IDENTITY_PATH = "usr/share/doc/kodosi/provenance/source-identity.json"
ARTIFACT_SUFFIXES = (".deb", ".tar.gz", ".pkg.tar.zst")
LINUX_GHOSTTY_REF_PATH = Path("LinuxGhostty.ref")
GHOSTTY_ARCHIVE_PATH = Path(
    "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
)


def run(
    arguments: list[str],
    *,
    cwd: Path | None = None,
    input_bytes: bytes | None = None,
) -> bytes:
    result = subprocess.run(
        arguments,
        cwd=cwd,
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(detail or f"{arguments[0]} failed")
    return result.stdout


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n"
    ).encode("utf-8")


def file_sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def repository_from_remote(source_root: Path) -> str:
    remote = run(
        ["git", "remote", "get-url", "origin"],
        cwd=source_root,
    ).decode("utf-8").strip()
    patterns = (
        r"^(?:https?://|ssh://git@)github\.com/(?P<repo>[^/]+/[^/]+?)(?:\.git)?$",
        r"^git@github\.com:(?P<repo>[^/]+/[^/]+?)(?:\.git)?$",
    )
    for pattern in patterns:
        match = re.fullmatch(pattern, remote)
        if match:
            return match.group("repo")
    raise ValueError("origin must identify a GitHub owner/name repository")


def _object_format(source_root: Path) -> tuple[str, int]:
    name = run(
        ["git", "rev-parse", "--show-object-format"],
        cwd=source_root,
    ).decode("ascii").strip()
    if name == "sha1":
        return name, 40
    if name == "sha256":
        return name, 64
    raise ValueError(f"unsupported Git object format: {name}")


def _blob_hash(
    algorithm: str,
    size: int,
    chunks,
) -> str:
    digest = hashlib.new(algorithm)
    digest.update(f"blob {size}\0".encode("ascii"))
    for chunk in chunks:
        digest.update(chunk)
    return digest.hexdigest()


def _worktree_blob(
    root: bytes,
    relative: bytes,
    expected_mode: bytes,
    algorithm: str,
) -> tuple[str, bool]:
    path = os.path.join(root, relative)
    try:
        before = os.lstat(path)
    except OSError:
        return "", True
    if expected_mode == b"120000":
        if not stat.S_ISLNK(before.st_mode):
            return "", True
        try:
            target = os.readlink(path)
            after = os.lstat(path)
        except OSError:
            return "", True
        if not isinstance(target, bytes):
            target = os.fsencode(target)
        changed = (
            (after.st_dev, after.st_ino) != (before.st_dev, before.st_ino)
            or after.st_mtime_ns != before.st_mtime_ns
            or after.st_ctime_ns != before.st_ctime_ns
        )
        return _blob_hash(algorithm, len(target), (target,)), changed
    if expected_mode not in {b"100644", b"100755"}:
        raise ValueError(
            f"unsupported tracked entry mode {expected_mode.decode('ascii')}"
        )
    if not stat.S_ISREG(before.st_mode):
        return "", True
    executable = bool(before.st_mode & stat.S_IXUSR)
    if executable != (expected_mode == b"100755"):
        return "", True
    flags = os.O_RDONLY
    flags |= getattr(os, "O_CLOEXEC", 0)
    flags |= getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError:
        return "", True
    try:
        opened = os.fstat(descriptor)
        if (
            not stat.S_ISREG(opened.st_mode)
            or (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino)
        ):
            return "", True

        read_size = 0

        def chunks():
            nonlocal read_size
            while True:
                chunk = os.read(descriptor, 1024 * 1024)
                if not chunk:
                    return
                read_size += len(chunk)
                yield chunk

        digest = _blob_hash(algorithm, opened.st_size, chunks())
        after = os.fstat(descriptor)
        changed = (
            read_size != opened.st_size
            or after.st_size != opened.st_size
            or after.st_mtime_ns != opened.st_mtime_ns
            or after.st_ctime_ns != opened.st_ctime_ns
            or after.st_mode != opened.st_mode
        )
        return digest, changed
    finally:
        os.close(descriptor)


def _checkout_dirty(source_root: Path) -> bool:
    algorithm, object_length = _object_format(source_root)
    tree: dict[bytes, tuple[bytes, bytes]] = {}
    for record in run(
        ["git", "ls-tree", "-rz", "--full-tree", "HEAD"],
        cwd=source_root,
    ).split(b"\0"):
        if not record:
            continue
        try:
            metadata, relative = record.split(b"\t", 1)
            mode, object_type, object_id = metadata.split(b" ", 2)
        except ValueError as error:
            raise ValueError("Git HEAD tree listing is malformed") from error
        if (
            not relative
            or relative.startswith(b"/")
            or any(part in {b"", b".", b".."} for part in relative.split(b"/"))
        ):
            raise ValueError("Git HEAD contains an unsafe tracked path")
        if object_type == b"commit" or mode == b"160000":
            raise ValueError(
                "Git submodules are not supported in release source checkouts"
            )
        if object_type != b"blob" or mode not in {b"100644", b"100755", b"120000"}:
            raise ValueError("Git HEAD contains an unsupported tracked entry")
        if (
            len(object_id) != object_length
            or not re.fullmatch(rb"[0-9a-f]+", object_id)
        ):
            raise ValueError("Git HEAD contains a malformed object identifier")
        if relative in tree:
            raise ValueError("Git HEAD contains duplicate tracked paths")
        tree[relative] = (mode, object_id)

    dirty = False
    index: dict[bytes, tuple[bytes, bytes]] = {}
    for record in run(
        ["git", "ls-files", "--stage", "-z"],
        cwd=source_root,
    ).split(b"\0"):
        if not record:
            continue
        try:
            metadata, relative = record.split(b"\t", 1)
            mode, object_id, stage = metadata.split(b" ", 2)
        except ValueError as error:
            raise ValueError("Git index listing is malformed") from error
        if stage != b"0":
            dirty = True
            continue
        if relative in index:
            dirty = True
        index[relative] = (mode, object_id)
    if index != tree:
        dirty = True

    for record in run(
        ["git", "ls-files", "-v", "-z"],
        cwd=source_root,
    ).split(b"\0"):
        if not record:
            continue
        if len(record) < 3 or record[1:2] != b" ":
            raise ValueError("Git index flag listing is malformed")
        tag = record[:1]
        if tag == b"S" or tag.islower():
            dirty = True

    if run(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"],
        cwd=source_root,
    ):
        dirty = True

    root = os.fsencode(source_root)
    for relative, (mode, expected_object) in tree.items():
        measured_object, changed = _worktree_blob(
            root,
            relative,
            mode,
            algorithm,
        )
        if changed or measured_object.encode("ascii") != expected_object:
            dirty = True
    return dirty


def repository_root(source_root: Path) -> Path:
    return Path(run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=source_root.resolve(strict=True),
    ).decode("utf-8").strip()).resolve(strict=True)


def checkout_identity(source_root: Path) -> dict[str, object]:
    root = repository_root(source_root)
    _, object_length = _object_format(root)
    commit = run(["git", "rev-parse", "--verify", "HEAD"], cwd=root).decode(
        "ascii"
    ).strip()
    if not re.fullmatch(rf"[0-9a-f]{{{object_length}}}", commit):
        raise ValueError("source HEAD is not a full lowercase Git commit")
    return {
        "repository": repository_from_remote(root),
        "commit": commit,
        "dirty": _checkout_dirty(root),
    }


def commit_epoch(source_root: Path) -> int:
    value = run(
        ["git", "show", "-s", "--format=%ct", "HEAD"],
        cwd=source_root.resolve(strict=True),
    ).decode("ascii").strip()
    if not re.fullmatch(r"[0-9]+", value):
        raise ValueError("source commit timestamp is malformed")
    epoch = int(value)
    if epoch <= 0:
        raise ValueError("source commit timestamp is invalid")
    return epoch


def validate_checkout_identity(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != {
        "repository",
        "commit",
        "dirty",
    }:
        raise ValueError("checkout identity fields are malformed")
    if not isinstance(value["repository"], str) or not re.fullmatch(
        r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+",
        value["repository"],
    ):
        raise ValueError("checkout identity repository is malformed")
    if not isinstance(value["commit"], str) or not re.fullmatch(
        r"(?:[0-9a-f]{40}|[0-9a-f]{64})",
        value["commit"],
    ):
        raise ValueError("checkout identity commit is malformed")
    if not isinstance(value["dirty"], bool):
        raise ValueError("checkout identity dirty flag is malformed")
    return value


def source_identity(source_root: Path) -> dict[str, object]:
    root = repository_root(source_root)
    return {
        "schemaVersion": 4,
        "sourceDateEpoch": commit_epoch(root),
        **checkout_identity(root),
    }


def validate_source_identity(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != {
        "schemaVersion", "sourceDateEpoch", "repository", "commit", "dirty",
    }:
        raise ValueError("source identity fields are malformed")
    if value["schemaVersion"] != 4:
        raise ValueError("unsupported source identity schema")
    if (
        not isinstance(value["sourceDateEpoch"], int)
        or isinstance(value["sourceDateEpoch"], bool)
        or value["sourceDateEpoch"] <= 0
    ):
        raise ValueError("source identity epoch is malformed")
    validate_checkout_identity({
        key: value[key] for key in ("repository", "commit", "dirty")
    })
    return value


def ghostty_measurements(ghostty_package_root: Path) -> dict[str, str]:
    root = ghostty_package_root.resolve(strict=True)
    ref_path = root / LINUX_GHOSTTY_REF_PATH
    archive_path = root / GHOSTTY_ARCHIVE_PATH
    try:
        ref_payload = ref_path.read_bytes()
    except OSError as error:
        raise ValueError(f"cannot read {ref_path}: {error}") from error
    if ref_payload.endswith(b"\n"):
        ref_payload = ref_payload[:-1]
    if not re.fullmatch(rb"[0-9a-f]{40}", ref_payload):
        raise ValueError(
            "LinuxGhostty.ref must contain one lowercase commit and newline"
        )
    if ref_path.read_bytes() != ref_payload + b"\n":
        raise ValueError(
            "LinuxGhostty.ref must contain one lowercase commit and newline"
        )
    if archive_path.is_symlink() or not archive_path.is_file():
        raise ValueError("pinned Linux libghostty-vt archive is not a regular file")
    return {
        "linuxUpstreamCommit": ref_payload.decode("ascii"),
        "linuxVtArchiveSha256": file_sha256(archive_path),
    }


def verify_release_sources(
    source_root: Path,
    *,
    require_clean: bool,
) -> tuple[dict[str, object], dict[str, str]]:
    root = repository_root(source_root)
    measured = source_identity(root)
    if require_clean and measured["dirty"]:
        raise ValueError("Kodosi checkout is dirty")
    lock = dict(
        line.split("=", 1)
        for line in (root / "Ghostty.lock").read_text(encoding="utf-8").splitlines()
        if line
    )
    measurements = ghostty_measurements(root / "terminal/ghostty")
    if measurements["linuxUpstreamCommit"] != lock.get("linux_upstream_commit"):
        raise ValueError("LinuxGhostty.ref differs from Ghostty.lock")
    if measurements["linuxVtArchiveSha256"] != lock.get("linux_vt_archive_sha256"):
        raise ValueError("Linux libghostty-vt archive differs from Ghostty.lock")
    return measured, measurements


def verify_reproducible_artifact(artifact: Path, source_date_epoch: int) -> None:
    run(
        [
            sys.executable,
            str(Path(__file__).with_name("verify-reproducible-archive.py")),
            "--artifact",
            str(artifact),
            "--source-date-epoch",
            str(source_date_epoch),
        ]
    )


def product_version(source_root: Path) -> str:
    cmake = (source_root / "CMakeLists.txt").read_text(encoding="utf-8")
    match = re.search(
        r"\bproject\s*\(\s*KodosiQT\b[\s\S]*?\bVERSION\s+"
        r"([0-9]+\.[0-9]+\.[0-9]+)\b",
        cmake,
    )
    if not match:
        raise ValueError("KodosiQT product version was not found")
    return match.group(1)


def artifact_kind(name: str) -> str:
    if name.endswith(".deb"):
        return "deb"
    if name.endswith(".tar.gz"):
        return "tgz"
    if name.endswith(".pkg.tar.zst"):
        return "arch"
    raise ValueError(f"unsupported artifact type: {name}")


def _reject_archive_text(value: str, description: str) -> None:
    if "\\" in value or any(
        unicodedata.category(character) in {"Cc", "Cs"} for character in value
    ):
        raise ValueError(f"unsafe {description}: {value!r}")


def normalize_archive_path(name: str, *, allow_root: bool = False) -> str:
    _reject_archive_text(name, "archive path")
    candidate = name[2:] if name.startswith("./") else name
    if candidate in {"", "."}:
        if allow_root:
            return "."
        raise ValueError(f"unsafe archive path: {name!r}")
    if posixpath.isabs(candidate) or PurePosixPath(candidate).is_absolute():
        raise ValueError(f"unsafe archive path: {name!r}")
    raw_parts = candidate.split("/")
    if any(part in {".", ".."} for part in raw_parts):
        raise ValueError(f"unsafe archive path: {name!r}")
    normalized = posixpath.normpath(candidate)
    path = PurePosixPath(normalized)
    if (
        normalized in {"", ".", ".."}
        or path.is_absolute()
        or any(part in {".", ".."} for part in path.parts)
    ):
        raise ValueError(f"unsafe archive path: {name!r}")
    return path.as_posix()


def source_identity_member_path(
    normalized: str,
    *,
    allow_package_root: bool,
) -> bool:
    if normalized == SOURCE_IDENTITY_PATH:
        return True
    expected = PurePosixPath(SOURCE_IDENTITY_PATH).parts
    parts = PurePosixPath(normalized).parts
    return allow_package_root and len(parts) == len(expected) + 1 and (
        parts[1:] == expected
    )


def _deb_tar(arguments: list[str], artifact: Path) -> bytes:
    producer = subprocess.Popen(
        ["dpkg-deb", "--fsys-tarfile", str(artifact)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert producer.stdout is not None
    consumer = subprocess.run(
        ["tar", *arguments],
        stdin=producer.stdout,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    producer.stdout.close()
    producer_stderr = producer.communicate()[1]
    if producer.returncode != 0 or consumer.returncode != 0:
        detail = (
            producer_stderr + consumer.stderr
        ).decode("utf-8", errors="replace").strip()
        raise ValueError(detail or "could not read DEB data archive")
    return consumer.stdout


def extract_source_identity(artifact: Path) -> dict[str, object]:
    if artifact.name.endswith(".deb"):
        listing = _deb_tar(["-tf", "-"], artifact).decode("utf-8").splitlines()
        matches = []
        for name in listing:
            normalized = normalize_archive_path(name, allow_root=True)
            if source_identity_member_path(
                normalized,
                allow_package_root=False,
            ):
                matches.append(name)
        if len(matches) != 1:
            raise ValueError(
                f"{artifact.name} must contain exactly one source identity"
            )
        payload = _deb_tar(["-xOf", "-", matches[0]], artifact)
    else:
        compression = ["--zstd"] if artifact.name.endswith(".pkg.tar.zst") else ["-z"]
        listing = run(
            ["tar", *compression, "-tf", str(artifact)]
        ).decode("utf-8").splitlines()
        matches = []
        for name in listing:
            try:
                normalized = normalize_archive_path(name, allow_root=True)
            except ValueError as error:
                raise ValueError(
                    f"{artifact.name} contains {error}"
                ) from error
            if source_identity_member_path(
                normalized,
                allow_package_root=artifact.name.endswith(".tar.gz"),
            ):
                matches.append(name)
        if len(matches) != 1:
            raise ValueError(
                f"{artifact.name} must contain exactly one source identity"
            )
        payload = run(
            ["tar", *compression, "-xOf", str(artifact), matches[0]]
        )
    try:
        value = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(
            f"{artifact.name} source identity is invalid JSON: {error}"
        ) from error
    if payload != canonical_json(value):
        raise ValueError(f"{artifact.name} source identity is not canonical")
    return validate_source_identity(value)
