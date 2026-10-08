#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


MAC_REF = Path("MacOSGhostty.ref")
LINUX_REF = Path("LinuxGhostty.ref")
MAC_METADATA = Path("Vendor/libghostty.version")
LINUX_METADATA = Path("Vendor/GhosttyVt/linux-x86_64.version")
MAC_ARCHIVE = Path(
    "Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
)
MAC_VT_ARCHIVE = Path("Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a")
LINUX_ARCHIVE = Path(
    "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
)
MAC_INVENTORY = Path("ThirdPartyNotices/inventory.json")
LINUX_INVENTORY = Path("ThirdPartyNotices/linux-vt-inventory.json")


def fail(message: str) -> None:
    raise SystemExit(f"platform pin verification error: {message}")


def read_ref(root: Path, relative: Path) -> str:
    path = root / relative
    try:
        payload = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {relative}: {error}")
    if not re.fullmatch(rb"[0-9a-f]{40}\n", payload):
        fail(f"{relative} must contain one lowercase commit and newline")
    return payload[:-1].decode("ascii")


def read_metadata(root: Path, relative: Path) -> dict[str, str]:
    path = root / relative
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail(f"cannot read {relative}: {error}")
    values: dict[str, str] = {}
    for line in lines:
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            fail(f"{relative} contains malformed metadata")
        values[key] = value
    return values


def read_inventory(root: Path, relative: Path) -> dict:
    try:
        value = json.loads((root / relative).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {relative}: {error}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain a JSON object")
    return value


def file_sha256(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"artifact is not a regular file: {relative}")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_digest(metadata: dict[str, str], key: str, relative: Path) -> str:
    value = metadata.get(key, "")
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        fail(f"{relative} has malformed {key}")
    return value


def verify(root: Path) -> None:
    mac_ref = read_ref(root, MAC_REF)
    linux_ref = read_ref(root, LINUX_REF)
    mac_metadata = read_metadata(root, MAC_METADATA)
    linux_metadata = read_metadata(root, LINUX_METADATA)
    mac_inventory = read_inventory(root, MAC_INVENTORY)
    linux_inventory = read_inventory(root, LINUX_INVENTORY)

    if mac_metadata.get("ghostty_commit") != mac_ref:
        fail("macOS artifact metadata differs from MacOSGhostty.ref")
    if mac_inventory.get("ghosttyCommit") != mac_ref:
        fail("macOS notice inventory differs from MacOSGhostty.ref")
    mac_digest = require_digest(
        mac_metadata,
        "combined_archive_sha256",
        MAC_METADATA,
    )
    if file_sha256(root, MAC_ARCHIVE) != mac_digest:
        fail("macOS combined archive differs from its recorded digest")
    if file_sha256(root, MAC_VT_ARCHIVE) != mac_digest:
        fail("macOS VT archive differs from the combined archive digest")

    if linux_metadata.get("ghostty_commit") != linux_ref:
        fail("Linux artifact metadata differs from LinuxGhostty.ref")
    if linux_inventory.get("ghosttyCommit") != linux_ref:
        fail("Linux notice inventory differs from LinuxGhostty.ref")
    linux_digest = require_digest(
        linux_metadata,
        "vt_archive_sha256",
        LINUX_METADATA,
    )
    if file_sha256(root, LINUX_ARCHIVE) != linux_digest:
        fail("Linux VT archive differs from its recorded digest")

    print(
        "Verified platform Ghostty pins and artifact digests: "
        f"macOS={mac_ref} Linux={linux_ref}"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
    )
    arguments = parser.parse_args()
    verify(arguments.root.resolve(strict=True))


if __name__ == "__main__":
    main()
