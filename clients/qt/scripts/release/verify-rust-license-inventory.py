#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from release_common import canonical_json, file_sha256


EVIDENCE_NAME = re.compile(r"^(?:licen[cs]e|copying|notice)", re.I)


def fail(message: str) -> None:
    raise SystemExit(f"Rust license verification error: {message}")


def relative_file(tree: Path, row: object) -> Path:
    if (
        not isinstance(row, dict)
        or set(row) != {"path", "sha256"}
        or not isinstance(row["path"], str)
        or not isinstance(row["sha256"], str)
        or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
    ):
        fail("file evidence row is malformed")
    relative = Path(row["path"])
    if relative.is_absolute() or ".." in relative.parts:
        fail("file evidence path contains traversal")
    candidate = (tree / relative).resolve(strict=True)
    if tree not in candidate.parents or not candidate.is_file():
        fail(f"file evidence escaped the inventory: {relative}")
    if file_sha256(candidate) != row["sha256"]:
        fail(f"file evidence digest mismatch: {relative}")
    return candidate


def load_override_manifest(tree: Path, row: object) -> tuple[dict, Path]:
    path = relative_file(tree, row)
    payload = path.read_bytes()
    try:
        value = json.loads(payload)
    except json.JSONDecodeError as error:
        fail(f"override manifest is invalid JSON: {error}")
    if payload != canonical_json(value):
        fail("override manifest is not canonical")
    if (
        not isinstance(value, dict)
        or set(value) != {"schemaVersion", "overrides"}
        or value["schemaVersion"] != 1
        or not isinstance(value["overrides"], list)
    ):
        fail("override manifest header is malformed")
    return value, path


def validate(tree: Path) -> dict:
    tree = tree.resolve(strict=True)
    inventory_path = tree / "rust-dependency-licenses.json"
    payload = inventory_path.read_bytes()
    try:
        value = json.loads(payload)
    except json.JSONDecodeError as error:
        fail(f"inventory JSON is invalid: {error}")
    if payload != canonical_json(value):
        fail("inventory JSON is not canonical")
    if (
        not isinstance(value, dict)
        or set(value)
        != {
            "schemaVersion",
            "target",
            "roots",
            "overrideManifest",
            "dependencies",
        }
        or value["schemaVersion"] != 2
        or value["target"] != "x86_64-unknown-linux-gnu"
        or value["roots"] != ["kodosi-ffi-c", "kodosi-runtime"]
        or not isinstance(value["dependencies"], list)
        or not value["dependencies"]
    ):
        fail("inventory header is malformed")

    override_document, override_path = load_override_manifest(
        tree,
        value["overrideManifest"],
    )
    overrides: dict[tuple[str, str, str], dict] = {}
    for row in override_document["overrides"]:
        if (
            not isinstance(row, dict)
            or set(row)
            != {
                "name",
                "version",
                "source",
                "license",
                "file",
                "sha256",
                "sourceUrl",
                "sourceRef",
                "sourceCommit",
            }
            or not all(isinstance(item, str) and item for item in row.values())
            or not re.fullmatch(r"[0-9a-f]{40}", row["sourceCommit"])
            or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
            or not row["sourceUrl"].startswith("https://")
            or row["sourceCommit"] not in row["sourceUrl"]
        ):
            fail("override provenance row is malformed")
        key = (row["name"], row["version"], row["source"])
        if key in overrides:
            fail("override provenance identities are not unique")
        overrides[key] = row

    expected_files = {inventory_path.resolve(), override_path.resolve()}
    identities = []
    used_overrides: set[tuple[str, str, str]] = set()
    for dependency in value["dependencies"]:
        if (
            not isinstance(dependency, dict)
            or set(dependency)
            != {
                "name",
                "version",
                "license",
                "source",
                "repository",
                "licenseFiles",
            }
            or not all(
                isinstance(dependency[field], str) and dependency[field]
                for field in ("name", "version", "license", "source")
            )
            or (
                dependency["repository"] is not None
                and not isinstance(dependency["repository"], str)
            )
            or not isinstance(dependency["licenseFiles"], list)
            or not dependency["licenseFiles"]
        ):
            fail("dependency row is malformed or metadata-only")
        identity = (
            dependency["name"],
            dependency["version"],
            dependency["source"],
        )
        identities.append((identity[0].casefold(), identity[1], identity[2]))
        for evidence in dependency["licenseFiles"]:
            if (
                not isinstance(evidence, dict)
                or set(evidence) != {"path", "sha256", "origin"}
            ):
                fail("license evidence row is malformed")
            path = relative_file(
                tree,
                {"path": evidence["path"], "sha256": evidence["sha256"]},
            )
            expected_files.add(path)
            if not EVIDENCE_NAME.match(path.name):
                fail(f"evidence is not LICENSE/COPYING/NOTICE text: {path.name}")
            if path.stat().st_size <= 0:
                fail(f"license evidence is empty: {path.name}")
            origin = evidence["origin"]
            if not isinstance(origin, dict) or "kind" not in origin:
                fail("license evidence origin is malformed")
            if origin["kind"] == "crate":
                if (
                    set(origin) != {"kind", "cratePath"}
                    or not isinstance(origin["cratePath"], str)
                    or not origin["cratePath"]
                    or not EVIDENCE_NAME.match(Path(origin["cratePath"]).name)
                ):
                    fail("crate license origin is malformed")
            elif origin["kind"] == "override":
                if set(origin) != {
                    "kind",
                    "sourceUrl",
                    "sourceRef",
                    "sourceCommit",
                }:
                    fail("override license origin is malformed")
                override = overrides.get(identity)
                if override is None:
                    fail(f"override provenance is missing for {identity}")
                if (
                    dependency["license"] != override["license"]
                    or evidence["sha256"] != override["sha256"]
                    or origin["sourceUrl"] != override["sourceUrl"]
                    or origin["sourceRef"] != override["sourceRef"]
                    or origin["sourceCommit"] != override["sourceCommit"]
                ):
                    fail(f"override provenance differs for {identity}")
                used_overrides.add(identity)
            else:
                fail("license evidence origin kind is unsupported")
    if identities != sorted(identities) or len(identities) != len(set(identities)):
        fail("dependency rows are not deterministic and unique")
    if used_overrides != set(overrides):
        fail("override provenance contains unused or missing identities")
    actual_files = {
        path.resolve()
        for path in tree.rglob("*")
        if path.is_file()
    }
    if actual_files != expected_files:
        fail("inventory contains unlisted or missing evidence files")
    return value


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tree", required=True, type=Path)
    parser.add_argument("--expected-tree", type=Path)
    arguments = parser.parse_args()
    actual = validate(arguments.tree)
    if arguments.expected_tree is not None:
        expected = validate(arguments.expected_tree)
        if actual != expected:
            fail("packaged inventory differs from generated inventory")
        for expected_file in arguments.expected_tree.resolve().rglob("*"):
            if not expected_file.is_file():
                continue
            relative = expected_file.relative_to(arguments.expected_tree.resolve())
            actual_file = arguments.tree.resolve() / relative
            if file_sha256(actual_file) != file_sha256(expected_file):
                fail(f"packaged evidence differs: {relative}")
    print(
        f"Rust dependency licenses verified: "
        f"{len(actual['dependencies'])} packages"
    )


if __name__ == "__main__":
    main()
