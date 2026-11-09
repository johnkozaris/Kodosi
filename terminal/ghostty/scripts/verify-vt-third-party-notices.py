#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True

import digest  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
NOTICE_INVENTORY_PATH = ROOT / "ThirdPartyNotices" / "inventory.json"
LAYOUTS = {
    "linux-x86_64": ("Linux", "linux-vt-inventory.json", "LinuxGhostty.ref"),
    "android-arm64": ("Android", "android-vt-inventory.json", "AndroidGhostty.ref"),
}


def fail(message: str) -> None:
    raise SystemExit(f"[!] VT third-party compliance: {message}")


def archive_members(path: Path) -> list[str]:
    try:
        data = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {path}: {error}")
    if not data.startswith(b"!<arch>\n"):
        fail(f"{path} is not an ar archive")
    members: list[str] = []
    long_names = b""
    offset = 8
    while offset + 60 <= len(data):
        header = data[offset:offset + 60]
        name, size = header[:16].rstrip(b" "), int(header[48:58])
        body = data[offset + 60:offset + 60 + size]
        if name == b"//":
            long_names = body
        elif name.startswith(b"/") and name[1:].isdigit():
            start = int(name[1:])
            members.append(long_names[start:long_names.index(b"/\n", start)].decode())
        elif name not in (b"/", b"/SYM64/"):
            members.append(name.removesuffix(b"/").decode())
        offset += 60 + size + size % 2
    return members


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")


def load_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail(f"cannot read {path}: {error}")
    for line in lines:
        if "=" not in line:
            fail("malformed VT artifact metadata")
        key, value = line.split("=", 1)
        if key in values:
            fail(f"duplicate metadata key: {key}")
        values[key] = value
    return values


def main() -> None:
    parser = argparse.ArgumentParser(description="Verify the notices of a Linux or Android Ghostty VT artifact.")
    parser.add_argument(
        "--vendor",
        type=Path,
        default=ROOT / "Vendor",
        help="the folder of the vendored artifacts. The default is Vendor in the Ghostty folder.",
    )
    parser.add_argument("--layout", choices=sorted(LAYOUTS), default="linux-x86_64")
    arguments = parser.parse_args()
    vendor, layout = arguments.vendor, arguments.layout
    platform, inventory_name, ref_name = LAYOUTS[layout]
    artifact_root = vendor / "GhosttyVt" / layout
    inventory = load_json(ROOT / "ThirdPartyNotices" / inventory_name)
    notice_inventory = load_json(NOTICE_INVENTORY_PATH)
    metadata = load_metadata(vendor / "GhosttyVt" / f"{layout}.version")
    if inventory.get("schemaVersion") != 1:
        fail("unsupported inventory schema")
    try:
        platform_ref = (ROOT / ref_name).read_text(encoding="ascii")
    except OSError as error:
        fail(f"cannot read {ref_name}: {error}")
    if platform_ref != f"{inventory.get('ghosttyCommit')}\n":
        fail(f"{ref_name} differs from the notice inventory")
    zig = inventory.get("zig", {})
    if (
        metadata.get("zig_version") != zig.get("version")
        or metadata.get("zig_target") != zig.get("target")
        or metadata.get("zig_sha256") != zig.get("sha256")
    ):
        fail("Zig toolchain identity differs from artifact metadata")

    static_archive = artifact_root / "lib" / "libghostty-vt.a"
    for artifact in inventory.get("artifacts", {}).values():
        if not (artifact_root / "lib" / Path(artifact).name).is_file():
            fail(f"{platform} VT artifact is missing: {artifact}")

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
            if not path.is_file() or digest.file_sha256(path) != record["sha256"]:
                fail(f"notice changed or is missing: {notice}")

    member_rows = inventory.get("archiveMembers")
    if not isinstance(member_rows, list) or not member_rows:
        fail("archive member inventory is empty")
    expected_members = [row["name"] for row in member_rows]
    actual_members = archive_members(static_archive)
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
        f"[*] verified {platform} VT notices for {len(component_ids)} components "
        f"and {len(actual_members)} archive members"
    )


if __name__ == "__main__":
    main()
