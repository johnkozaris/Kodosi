#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
INVENTORY_PATH = ROOT / "ThirdPartyNotices" / "linux-vt-inventory.json"
NOTICE_INVENTORY_PATH = ROOT / "ThirdPartyNotices" / "inventory.json"
ARTIFACT_ROOT = Path(
    os.environ.get(
        "KODOSI_GHOSTTY_LINUX_ARTIFACT_ROOT",
        ROOT / "Vendor" / "GhosttyVt" / "linux-x86_64",
    )
)
METADATA_PATH = Path(
    os.environ.get(
        "KODOSI_GHOSTTY_LINUX_METADATA",
        ROOT / "Vendor" / "GhosttyVt" / "linux-x86_64.version",
    )
)


def fail(message: str) -> None:
    raise SystemExit(f"[!] Linux VT third-party compliance: {message}")


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")


def load_metadata() -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = METADATA_PATH.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail(f"cannot read {METADATA_PATH}: {error}")
    for line in lines:
        if "=" not in line:
            fail("malformed Linux artifact metadata")
        key, value = line.split("=", 1)
        if key in values:
            fail(f"duplicate metadata key: {key}")
        values[key] = value
    return values


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    inventory = load_json(INVENTORY_PATH)
    notice_inventory = load_json(NOTICE_INVENTORY_PATH)
    metadata = load_metadata()
    if inventory.get("schemaVersion") != 1:
        fail("unsupported inventory schema")
    try:
        platform_ref = (ROOT / "LinuxGhostty.ref").read_text(encoding="ascii")
    except OSError as error:
        fail(f"cannot read LinuxGhostty.ref: {error}")
    if platform_ref != f"{inventory.get('ghosttyCommit')}\n":
        fail("LinuxGhostty.ref differs from the notice inventory")
    if metadata.get("ghostty_commit") != inventory.get("ghosttyCommit"):
        fail("Ghostty source identity differs from artifact metadata")
    zig = inventory.get("zig", {})
    if (
        metadata.get("zig_version") != zig.get("version")
        or metadata.get("zig_target") != zig.get("target")
        or metadata.get("zig_sha256") != zig.get("sha256")
    ):
        fail("Zig toolchain identity differs from artifact metadata")

    static_archive = ARTIFACT_ROOT / "lib" / "libghostty-vt.a"
    shared_library = ARTIFACT_ROOT / "lib" / "libghostty-vt.so.0.1.0"
    if not static_archive.is_file() or not shared_library.is_file():
        fail("Linux VT static or shared artifact is missing")

    component_rows = inventory.get("components")
    if not isinstance(component_rows, list) or not component_rows:
        fail("component inventory is empty")
    component_ids: set[str] = set()
    for component in component_rows:
        if set(component) != {"id", "identity", "spdx", "notices"}:
            fail(f"invalid component record: {component.get('id', '<unknown>')}")
        component_id = component["id"]
        if component_id in component_ids or not component["identity"]:
            fail(f"invalid component identity: {component_id}")
        component_ids.add(component_id)
        for notice in component["notices"]:
            if notice == "LICENSE-GHOSTTY":
                path = ROOT / notice
                if not path.is_file():
                    fail("Ghostty license is missing")
                continue
            record = notice_inventory.get("notices", {}).get(notice)
            if record is None:
                fail(f"component {component_id} references unknown notice {notice}")
            path = ROOT / record["path"]
            if not path.is_file() or sha256(path) != record["sha256"]:
                fail(f"notice changed or is missing: {notice}")

    member_rows = inventory.get("archiveMembers")
    if not isinstance(member_rows, list) or not member_rows:
        fail("archive member inventory is empty")
    expected_members = [row["name"] for row in member_rows]
    actual_members = subprocess.check_output(
        ["ar", "t", static_archive],
        text=True,
    ).splitlines()
    if actual_members != expected_members:
        fail(
            "archive member inventory changed "
            f"(expected={expected_members}, actual={actual_members})"
        )
    if any("/" in member for member in actual_members):
        fail("archive contains build-host paths")

    covered: set[str] = set()
    for row in member_rows:
        if set(row) != {"name", "owners"} or not row["owners"]:
            fail(f"invalid archive member record: {row}")
        unknown = set(row["owners"]) - component_ids
        if unknown:
            fail(f"archive member {row['name']} has unknown owners: {sorted(unknown)}")
        covered.update(row["owners"])
    if covered != component_ids:
        fail(f"components lack archive ownership: {sorted(component_ids - covered)}")

    print(
        f"[*] verified Linux VT notices for {len(component_ids)} components "
        f"and {len(actual_members)} archive members"
    )


if __name__ == "__main__":
    main()
