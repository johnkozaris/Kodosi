#!/usr/bin/env python3

from __future__ import annotations

import argparse
import io
import json
import posixpath
import re
import subprocess
import tarfile
import unicodedata
from pathlib import Path, PurePosixPath

from release_common import (
    normalize_archive_path,
    source_identity_member_path,
)


def fail(message: str) -> None:
    raise SystemExit(f"reproducible archive error: {message}")


def member_path(member: tarfile.TarInfo) -> str:
    try:
        return normalize_archive_path(member.name, allow_root=member.isdir())
    except ValueError as error:
        fail(str(error))


def validate_link_text(value: str, description: str) -> None:
    if "\\" in value or any(
        unicodedata.category(character) in {"Cc", "Cs"} for character in value
    ):
        fail(f"unsafe {description}: {value!r}")


def symlink_target(member_path: str, target: str) -> str:
    validate_link_text(target, "symlink target")
    if not target or posixpath.isabs(target):
        fail(f"unsafe symlink target: {target!r}")
    parent = posixpath.dirname(member_path)
    normalized = posixpath.normpath(posixpath.join(parent, target))
    path = PurePosixPath(normalized)
    if (
        normalized in {"", ".."}
        or path.is_absolute()
        or any(part == ".." for part in path.parts)
    ):
        fail(f"unsafe symlink target: {target!r}")
    return path.as_posix()


def hardlink_target(target: str) -> str:
    validate_link_text(target, "hardlink target")
    try:
        return normalize_archive_path(target)
    except ValueError:
        fail(f"unsafe hardlink target: {target!r}")


def validate_member(member: tarfile.TarInfo, epoch: int) -> str:
    name = member.name
    normalized = member_path(member)
    if not (
        member.isfile()
        or member.isdir()
        or member.issym()
        or member.islnk()
    ):
        fail(f"unsupported archive member type: {name}")
    if member.mtime != epoch:
        fail(f"archive mtime differs from SOURCE_DATE_EPOCH: {name}")
    if member.uid != 0 or member.gid != 0:
        fail(f"archive ownership is not root:root: {name}")
    if member.uname not in {"", "root"} or member.gname not in {"", "root"}:
        fail(f"archive owner names are not normalized: {name}")
    if any(key in member.pax_headers for key in ("atime", "ctime")):
        fail(f"archive contains host filesystem timestamps: {name}")
    mode = member.mode & 0o7777
    if member.isdir() and mode != 0o755:
        fail(f"archive directory mode is not 0755: {name}")
    if (member.isfile() or member.islnk()) and mode not in {0o644, 0o755}:
        fail(f"archive file mode is not normalized: {name}")
    if member.issym() and mode != 0o777:
        fail(f"archive symlink mode is not 0777: {name}")
    return normalized


def resolve_link_path(
    path: str,
    links: dict[str, str],
) -> str:
    current = path
    visited: set[str] = set()
    while True:
        parts = PurePosixPath(current).parts
        matched = False
        for length in range(1, len(parts) + 1):
            prefix = PurePosixPath(*parts[:length]).as_posix()
            target = links.get(prefix)
            if target is None:
                continue
            if prefix in visited:
                fail(f"archive link chain is cyclic: {path}")
            visited.add(prefix)
            remainder = parts[length:]
            current = (
                PurePosixPath(target, *remainder).as_posix()
                if remainder
                else target
            )
            if current == ".." or current.startswith("../"):
                fail(f"archive link chain escapes root: {path}")
            matched = True
            break
        if not matched:
            return current


def validate_tar(
    archive: tarfile.TarFile,
    epoch: int,
    *,
    require_pkginfo: bool,
    expected_orders: list[list[str]] | None = None,
) -> None:
    names: list[str] = []
    members: dict[str, tarfile.TarInfo] = {}
    links: dict[str, str] = {}
    pkginfo: bytes | None = None
    for member in archive:
        normalized = validate_member(member, epoch)
        names.append(member.name)
        if normalized in members:
            fail(f"archive contains duplicate normalized path: {normalized}")
        members[normalized] = member
        if member.issym():
            links[normalized] = symlink_target(normalized, member.linkname)
        elif member.islnk():
            links[normalized] = hardlink_target(member.linkname)
        if normalized == ".PKGINFO":
            if not member.isfile():
                fail(".PKGINFO is not a regular file")
            source = archive.extractfile(member)
            if source is None:
                fail(".PKGINFO is not a regular file")
            pkginfo = source.read()
    for normalized, member in members.items():
        if normalized != ".":
            parts = PurePosixPath(normalized).parts
            for length in range(1, len(parts)):
                parent = PurePosixPath(*parts[:length]).as_posix()
                parent_member = members.get(parent)
                if parent_member is not None and not parent_member.isdir():
                    fail(
                        "archive member is nested below a non-directory: "
                        f"{normalized}"
                    )
        if member.issym():
            resolve_link_path(links[normalized], links)
        elif member.islnk():
            target = links[normalized]
            if target not in members:
                fail(f"archive hardlink target is missing: {member.linkname}")
            resolved = resolve_link_path(target, links)
            target_member = members.get(resolved)
            if target_member is None or not target_member.isfile():
                fail(
                    f"archive hardlink target is not a regular file: "
                    f"{member.linkname}"
                )
    if expected_orders is not None:
        if names not in expected_orders:
            fail(
                "archive metadata member order is not deterministic: "
                + ", ".join(names)
            )
    elif names != sorted(names):
        fail("archive paths are not sorted")
    if any(PurePosixPath(name).name == ".BUILDINFO" for name in members):
        fail("Arch package contains host-derived .BUILDINFO")
    if require_pkginfo:
        if pkginfo is None:
            fail("Arch package is missing .PKGINFO")
        text = pkginfo.decode("utf-8")
        if f"builddate = {epoch}\n" not in text:
            fail("Arch package builddate differs from SOURCE_DATE_EPOCH")
        for forbidden in ("buildhost", "/home/", "user = ", "buildenv = "):
            if forbidden in text:
                fail(f"Arch metadata contains host-derived field: {forbidden}")


def parse_ar(payload: bytes, epoch: int) -> dict[str, bytes]:
    if not payload.startswith(b"!<arch>\n"):
        fail("DEB is not an ar archive")
    offset = 8
    members: dict[str, bytes] = {}
    order: list[str] = []
    while offset < len(payload):
        header = payload[offset : offset + 60]
        if len(header) != 60 or header[58:60] != b"`\n":
            fail("DEB ar header is malformed")
        name = header[:16].decode("ascii").strip().rstrip("/")
        try:
            mtime = int(header[16:28].decode("ascii").strip())
            uid = int(header[28:34].decode("ascii").strip())
            gid = int(header[34:40].decode("ascii").strip())
            mode = int(header[40:48].decode("ascii").strip(), 8)
            size = int(header[48:58].decode("ascii").strip())
        except ValueError as error:
            fail(f"DEB ar metadata is malformed: {error}")
        if mtime != epoch or uid != 0 or gid != 0:
            fail(f"DEB ar metadata is not normalized: {name}")
        if mode & 0o777 != 0o644:
            fail(f"DEB ar member mode is not 0644: {name}")
        offset += 60
        body = payload[offset : offset + size]
        if len(body) != size:
            fail(f"DEB ar member is truncated: {name}")
        if name in members:
            fail(f"DEB ar member is duplicated: {name}")
        members[name] = body
        order.append(name)
        offset += size + (size % 2)
    if (
        len(order) != 3
        or order[0] != "debian-binary"
        or not order[1].startswith("control.tar.")
        or not order[2].startswith("data.tar.")
    ):
        fail("DEB ar member set or order is not deterministic")
    if members["debian-binary"] != b"2.0\n":
        fail("DEB format marker is invalid")
    return members


def validate_deb(path: Path, epoch: int) -> None:
    members = parse_ar(path.read_bytes(), epoch)
    control_name = next(name for name in members if name.startswith("control.tar."))
    data_name = next(name for name in members if name.startswith("data.tar."))
    for name in (control_name, data_name):
        with tarfile.open(fileobj=io.BytesIO(members[name]), mode="r:*") as archive:
            validate_tar(
                archive,
                epoch,
                require_pkginfo=False,
                expected_orders=(
                    [
                        ["./md5sums", "./control"],
                        ["./control", "./md5sums"],
                        [".", "./control"],
                    ]
                    if name == control_name
                    else None
                ),
            )
    with tarfile.open(
        fileobj=io.BytesIO(members[control_name]),
        mode="r:*",
    ) as archive:
        control_member = archive.getmember("./control")
        source = archive.extractfile(control_member)
        if source is None:
            fail("DEB control metadata is missing")
        control = source.read().decode("utf-8")
    if re.search(r"^(Build-Date|Build-Host|Built-By):", control, re.MULTILINE):
        fail("DEB contains wall-clock or build-host metadata")


def validate_arch(path: Path, epoch: int) -> None:
    producer = subprocess.Popen(
        ["zstd", "--quiet", "--decompress", "--stdout", str(path)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert producer.stdout is not None
    try:
        with tarfile.open(fileobj=producer.stdout, mode="r|") as archive:
            validate_tar(archive, epoch, require_pkginfo=True)
    finally:
        producer.stdout.close()
    stderr = producer.communicate()[1]
    if producer.returncode != 0:
        fail(f"cannot decompress Arch package: {stderr.decode().strip()}")


def source_identity_payload(
    archive: tarfile.TarFile,
    *,
    allow_package_root: bool,
) -> bytes:
    payloads: list[bytes] = []
    for member in archive:
        normalized = member_path(member)
        if not source_identity_member_path(
            normalized,
            allow_package_root=allow_package_root,
        ):
            continue
        if not member.isfile():
            fail("archive source identity is not a regular file")
        source = archive.extractfile(member)
        if source is None:
            fail("archive source identity is unreadable")
        payloads.append(source.read())
    if len(payloads) != 1:
        fail("archive source identity is missing or duplicated")
    return payloads[0]


def verify_source_identity(path: Path, epoch: int) -> None:
    if path.name.endswith(".deb"):
        members = parse_ar(path.read_bytes(), epoch)
        data_name = next(name for name in members if name.startswith("data.tar."))
        with tarfile.open(
            fileobj=io.BytesIO(members[data_name]),
            mode="r:*",
        ) as archive:
            payload = source_identity_payload(
                archive,
                allow_package_root=False,
            )
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, mode="r:gz") as archive:
            payload = source_identity_payload(
                archive,
                allow_package_root=True,
            )
    else:
        producer = subprocess.Popen(
            ["zstd", "--quiet", "--decompress", "--stdout", str(path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        assert producer.stdout is not None
        try:
            with tarfile.open(fileobj=producer.stdout, mode="r|") as archive:
                payload = source_identity_payload(
                    archive,
                    allow_package_root=False,
                )
        finally:
            producer.stdout.close()
        stderr = producer.communicate()[1]
        if producer.returncode != 0:
            fail(f"cannot decompress Arch package: {stderr.decode().strip()}")
    try:
        identity = json.loads(payload)
    except json.JSONDecodeError as error:
        fail(f"source identity is invalid JSON: {error}")
    if identity.get("sourceDateEpoch") != epoch:
        fail("embedded source identity epoch differs from SOURCE_DATE_EPOCH")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", required=True, type=Path)
    parser.add_argument("--source-date-epoch", required=True, type=int)
    arguments = parser.parse_args()
    path = arguments.artifact.resolve(strict=True)
    epoch = arguments.source_date_epoch
    if epoch <= 0:
        fail("SOURCE_DATE_EPOCH must be positive")
    if path.name.endswith(".deb"):
        validate_deb(path, epoch)
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, mode="r:gz") as archive:
            validate_tar(archive, epoch, require_pkginfo=False)
    elif path.name.endswith(".pkg.tar.zst"):
        validate_arch(path, epoch)
    else:
        fail(f"unsupported artifact: {path.name}")
    verify_source_identity(path, epoch)
    print(f"reproducible archive metadata verified: {path.name}")


if __name__ == "__main__":
    main()
