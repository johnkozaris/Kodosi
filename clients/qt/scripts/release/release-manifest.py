#!/usr/bin/env python3
"""Write or check the release manifest of the Linux packages."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

from canonical_json import canonical_json, write_canonical_json
from release_source import product_version, repository_root, source_identity


ROOT = Path(__file__).resolve().parents[2]
NAME = "release-manifest.json"
KINDS = {".deb": "deb", ".tar.gz": "tgz", ".pkg.tar.zst": "arch"}
MAX_PACKAGE_BYTES = 2 * 1024 * 1024 * 1024
DIGEST = re.compile(r"[0-9a-f]{64}")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
FIELDS = {
    "productVersion": "product version",
    "target": "target",
    "source": "source identity",
    "pins": "pins",
}


def fail(message: str) -> None:
    raise SystemExit(f"release manifest error: {message}")


def file_sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def package_kind(name: str) -> str | None:
    return next((kind for suffix, kind in KINDS.items() if name.endswith(suffix)), None)


def checkout_fields() -> dict[str, object]:
    """The fields of the manifest that the clean checkout gives."""
    try:
        source = source_identity(ROOT)
        repository = repository_root(ROOT)
        dependencies = json.loads((ROOT / "dependencies.lock.json").read_bytes())
        contract = json.loads((repository / "protocol/desktop-runtime.json").read_bytes())
        lock = dict(
            line.split("=", 1)
            for line in (repository / "Ghostty.lock").read_text(encoding="utf-8").splitlines()
            if line
        )
        fields = {
            "productVersion": product_version(ROOT),
            "target": "linux-x86_64",
            "source": source,
            "pins": {
                "qtVersion": dependencies["frameworks"]["qt"]["version"],
                "runtime": {
                    "ffiAbiVersion": contract["ffiAbiVersion"],
                    "desktopProtocolVersion": contract["protocolVersion"],
                },
                "ghostty": {
                    "repository": dependencies["ghostty"]["repository"],
                    "commit": lock["linux_ghostty_commit"],
                    "linuxVtArchiveSha256": lock["linux_x86_64_archive_sha256"],
                },
                "dependenciesLockSha256": file_sha256(ROOT / "dependencies.lock.json"),
            },
        }
    except (KeyError, OSError, ValueError) as error:
        fail(f"cannot read the checkout: {error}")
    if source["dirty"]:
        fail("the checkout has changes. Commit all the changes first")
    return fields


def generate(packages: list[Path]) -> None:
    folders = {package.resolve().parent for package in packages}
    if len(folders) != 1:
        fail("put all the packages in one folder")
    folder = folders.pop()
    fields = checkout_fields()
    rows = []
    for package in sorted(packages, key=lambda package: package.name):
        if package.is_symlink() or not package.is_file() or package_kind(package.name) is None:
            fail(f"{package} is not a Linux package file")
        rows.append(
            {
                "filename": package.name,
                "size": package.stat().st_size,
                "sha256": file_sha256(package),
                "sourceIdentity": fields["source"],
            }
        )
    output = folder / NAME
    write_canonical_json(output, {"schemaVersion": 6, **fields, "artifacts": rows})
    print(output)


def validate_source(value: object) -> None:
    if not isinstance(value, dict) or set(value) != {
        "schemaVersion",
        "sourceDateEpoch",
        "repository",
        "commit",
        "dirty",
    }:
        fail("the source identity fields are malformed")
    epoch = value["sourceDateEpoch"]
    if (
        value["schemaVersion"] != 4
        or not isinstance(epoch, int)
        or isinstance(epoch, bool)
        or epoch <= 0
        or not isinstance(value["repository"], str)
        or not REPOSITORY.fullmatch(value["repository"])
        or not isinstance(value["commit"], str)
        or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", value["commit"])
    ):
        fail("the source identity is malformed")
    if value["dirty"] is not False:
        fail("the manifest records a checkout with changes")


def validate_digest(value: object, description: str) -> None:
    if not isinstance(value, str) or not DIGEST.fullmatch(value):
        fail(f"{description} is malformed")


def validate_structure(manifest: object) -> None:
    if not isinstance(manifest, dict) or set(manifest) != {
        "schemaVersion",
        *FIELDS,
        "artifacts",
    }:
        fail("the top-level fields differ from schema 6")
    if manifest["schemaVersion"] != 6:
        fail("the manifest schema is unsupported")
    if not isinstance(manifest["productVersion"], str) or not re.fullmatch(
        r"[0-9]+\.[0-9]+\.[0-9]+",
        manifest["productVersion"],
    ):
        fail("the product version is malformed")
    if manifest["target"] != "linux-x86_64":
        fail("the release target is unsupported")
    validate_source(manifest["source"])

    pins = manifest["pins"]
    if not isinstance(pins, dict) or set(pins) != {
        "qtVersion",
        "runtime",
        "ghostty",
        "dependenciesLockSha256",
    }:
        fail("the pins are malformed")
    runtime = pins["runtime"]
    ghostty = pins["ghostty"]
    if (
        not isinstance(pins["qtVersion"], str)
        or not pins["qtVersion"]
        or not isinstance(runtime, dict)
        or set(runtime) != {"ffiAbiVersion", "desktopProtocolVersion"}
        or not isinstance(ghostty, dict)
        or set(ghostty) != {"repository", "commit", "linuxVtArchiveSha256"}
    ):
        fail("the pins are malformed")
    if not isinstance(ghostty["repository"], str) or not REPOSITORY.fullmatch(ghostty["repository"]):
        fail("the Ghostty repository is malformed")
    if not isinstance(ghostty["commit"], str) or not re.fullmatch(r"[0-9a-f]{40}", ghostty["commit"]):
        fail("the Ghostty commit is malformed")
    if not all(
        isinstance(runtime[key], int) and not isinstance(runtime[key], bool) and runtime[key] > 0
        for key in runtime
    ):
        fail("the runtime versions are malformed")
    validate_digest(ghostty["linuxVtArchiveSha256"], "the Ghostty archive digest")
    validate_digest(pins["dependenciesLockSha256"], "the dependency lock digest")

    rows = manifest["artifacts"]
    if not isinstance(rows, list) or len(rows) != 3:
        fail("the manifest must list three packages")
    for row in rows:
        if not isinstance(row, dict) or set(row) != {"filename", "size", "sha256", "sourceIdentity"}:
            fail("a package row is malformed")
        name = row["filename"]
        if not isinstance(name, str) or Path(name).name != name or "\\" in name or name in {".", ".."}:
            fail("a package file name contains a path")
        if (
            not isinstance(row["size"], int)
            or isinstance(row["size"], bool)
            or not 0 < row["size"] <= MAX_PACKAGE_BYTES
        ):
            fail(f"the size of {name} is malformed")
        validate_digest(row["sha256"], f"the digest of {name}")
        if row["sourceIdentity"] != manifest["source"]:
            fail(f"the source identity of {name} differs from the source identity of the manifest")
    names = [row["filename"] for row in rows]
    if sorted(package_kind(name) or name for name in names) != ["arch", "deb", "tgz"]:
        fail("the manifest must list one Debian package, one portable archive and one Arch package")
    if names != sorted(names):
        fail("the package rows are not in name order")


def verify(manifest_path: Path, structure_only: bool) -> None:
    if manifest_path.is_symlink() or manifest_path.name != NAME:
        fail(f"the manifest must be a regular file with the name {NAME}")
    try:
        path = manifest_path.resolve(strict=True)
        payload = path.read_bytes()
        manifest = json.loads(payload.decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {manifest_path}: {error}")
    validate_structure(manifest)
    if payload != canonical_json(manifest):
        fail("the manifest is not canonical JSON")
    if structure_only:
        print("release manifest structure checked")
        return

    for key, value in checkout_fields().items():
        if manifest[key] != value:
            fail(f"the {FIELDS[key]} differs from the checkout")
    folder = path.parent
    names = {row["filename"] for row in manifest["artifacts"]}
    packages = {entry.name for entry in folder.iterdir() if package_kind(entry.name) is not None}
    if packages != names:
        fail(
            f"the packages in {folder} differ from the manifest: "
            f"missing {sorted(names - packages)}, extra {sorted(packages - names)}"
        )
    for row in manifest["artifacts"]:
        package = folder / row["filename"]
        if package.is_symlink() or not package.is_file():
            fail(f"{row['filename']} is not a regular file")
        if package.stat().st_size != row["size"] or file_sha256(package) != row["sha256"]:
            fail(f"{row['filename']} differs from its size and SHA-256 in the manifest")
    print(f"release manifest checked: {', '.join(sorted(names))}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    generate_command = commands.add_parser("generate", help=f"write {NAME} beside the packages")
    generate_command.add_argument("packages", nargs="+", type=Path)
    verify_command = commands.add_parser("verify", help="check a release manifest")
    verify_command.add_argument(
        "--structure-only",
        action="store_true",
        help="examine only the fields of the manifest, without the checkout and the packages",
    )
    verify_command.add_argument("manifest", type=Path)
    arguments = parser.parse_args()
    if arguments.command == "generate":
        generate(arguments.packages)
    else:
        verify(arguments.manifest, arguments.structure_only)


if __name__ == "__main__":
    main()
