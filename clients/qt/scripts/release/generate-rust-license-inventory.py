#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path

from release_common import canonical_json


ROOT = Path(__file__).resolve().parents[2]
MAX_EVIDENCE_BYTES = 2 * 1024 * 1024
EVIDENCE_NAME = re.compile(r"^(?:licen[cs]e|copying|notice)", re.I)


def fail(message: str) -> None:
    raise SystemExit(f"Rust license inventory error: {message}")


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_overrides(path: Path) -> tuple[dict[tuple[str, str, str], dict], bytes]:
    manifest_payload = path.read_bytes()
    try:
        value = json.loads(manifest_payload)
    except json.JSONDecodeError as error:
        fail(f"override manifest is invalid JSON: {error}")
    if manifest_payload != canonical_json(value):
        fail("override manifest is not canonical JSON")
    if (
        not isinstance(value, dict)
        or set(value) != {"schemaVersion", "overrides"}
        or value["schemaVersion"] != 1
        or not isinstance(value["overrides"], list)
    ):
        fail("override manifest header is malformed")
    result: dict[tuple[str, str, str], dict] = {}
    for row in value["overrides"]:
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
            fail("override manifest row is malformed")
        relative = Path(row["file"])
        if relative.is_absolute() or ".." in relative.parts:
            fail("override file path contains traversal")
        source = (path.parent / relative).resolve(strict=True)
        if path.parent.resolve() not in source.parents or not source.is_file():
            fail(f"override evidence escaped its directory: {relative}")
        evidence_payload = source.read_bytes()
        if (
            not evidence_payload
            or len(evidence_payload) > MAX_EVIDENCE_BYTES
            or digest_bytes(evidence_payload) != row["sha256"]
            or not EVIDENCE_NAME.match(source.name)
        ):
            fail(f"override evidence is invalid: {relative}")
        key = (row["name"], row["version"], row["source"])
        if key in result:
            fail(f"duplicate override identity: {key}")
        result[key] = row
    if list(result) != sorted(
        result,
        key=lambda item: (item[0].casefold(), item[1], item[2]),
    ):
        fail("override rows are not deterministically ordered")
    return result, manifest_payload


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument(
        "--overrides",
        type=Path,
        default=ROOT / "packaging/licenses/rust-license-overrides.json",
    )
    arguments = parser.parse_args()
    try:
        overrides, override_payload = load_overrides(arguments.overrides)
    except (OSError, ValueError) as error:
        fail(str(error))

    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
            "--manifest-path",
            str(arguments.manifest),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(result.stderr.decode("utf-8", errors="replace"))
    metadata = json.loads(result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [
        package["id"]
        for package in metadata["packages"]
        if package["source"] is None
        and package["name"] in {"kodosi-runtime", "kodosi-ffi-c"}
    ]
    if len(roots) != 2:
        fail("the runtime and FFI roots were not both resolved")

    closure: set[str] = set()
    pending = roots.copy()
    while pending:
        package_id = pending.pop()
        if package_id in closure:
            continue
        closure.add(package_id)
        for dependency in nodes[package_id]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])

    output_dir = arguments.output_dir
    shutil.rmtree(output_dir, ignore_errors=True)
    licenses_dir = output_dir / "licenses"
    licenses_dir.mkdir(parents=True)
    override_destination = output_dir / "rust-license-overrides.json"
    override_destination.write_bytes(override_payload)
    inventory: list[dict[str, object]] = []
    used_overrides: set[tuple[str, str, str]] = set()
    external = sorted(
        (
            packages[package_id]
            for package_id in closure
            if packages[package_id]["source"] is not None
        ),
        key=lambda package: (
            package["name"].casefold(),
            package["version"],
            package["source"],
        ),
    )
    for package in external:
        license_expression = package.get("license")
        if not isinstance(license_expression, str) or not license_expression.strip():
            fail(f"{package['name']} {package['version']} has no declared license")
        crate_root = Path(package["manifest_path"]).parent
        evidence = sorted(
            (
                path
                for path in crate_root.rglob("*")
                if path.is_file()
                and not path.is_symlink()
                and EVIDENCE_NAME.match(path.name)
            ),
            key=lambda path: str(path.relative_to(crate_root)).casefold(),
        )
        package_digest = digest_bytes(package["id"].encode("utf-8"))[:16]
        safe_name = re.sub(r"[^A-Za-z0-9_.-]", "_", package["name"])
        relative_root = Path(f"{safe_name}-{package['version']}-{package_digest}")
        evidence_rows: list[dict[str, object]] = []
        for source in evidence:
            payload = source.read_bytes()
            if not payload or len(payload) > MAX_EVIDENCE_BYTES:
                fail(f"invalid license evidence file: {source}")
            source_relative = source.relative_to(crate_root)
            destination = licenses_dir / relative_root / source_relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(payload)
            evidence_rows.append(
                {
                    "path": str(Path("licenses") / relative_root / source_relative),
                    "sha256": digest_bytes(payload),
                    "origin": {
                        "kind": "crate",
                        "cratePath": str(source_relative),
                    },
                }
            )
        if not evidence_rows:
            key = (package["name"], package["version"], package["source"])
            override = overrides.get(key)
            if override is None:
                fail(
                    f"{package['name']} {package['version']} has no actual "
                    "LICENSE/COPYING/NOTICE text"
                )
            if override["license"] != license_expression:
                fail(f"override license identity differs for {package['name']}")
            used_overrides.add(key)
            source = (
                arguments.overrides.parent / Path(override["file"])
            ).resolve(strict=True)
            payload = source.read_bytes()
            destination = licenses_dir / relative_root / source.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(payload)
            evidence_rows.append(
                {
                    "path": str(Path("licenses") / relative_root / source.name),
                    "sha256": digest_bytes(payload),
                    "origin": {
                        "kind": "override",
                        "sourceUrl": override["sourceUrl"],
                        "sourceRef": override["sourceRef"],
                        "sourceCommit": override["sourceCommit"],
                    },
                }
            )
        inventory.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": license_expression,
                "source": package["source"],
                "repository": package.get("repository"),
                "licenseFiles": evidence_rows,
            }
        )

    if used_overrides != set(overrides):
        unused = sorted(set(overrides) - used_overrides)
        fail(f"override manifest contains unused identities: {unused}")
    document = {
        "schemaVersion": 2,
        "target": "x86_64-unknown-linux-gnu",
        "roots": ["kodosi-ffi-c", "kodosi-runtime"],
        "overrideManifest": {
            "path": "rust-license-overrides.json",
            "sha256": digest_bytes(override_payload),
        },
        "dependencies": inventory,
    }
    (output_dir / "rust-dependency-licenses.json").write_bytes(
        canonical_json(document)
    )
    print(f"Rust dependency licenses: {len(inventory)} packages")


if __name__ == "__main__":
    main()
