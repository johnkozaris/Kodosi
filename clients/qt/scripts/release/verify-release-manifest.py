#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from release_common import (
    ARTIFACT_SUFFIXES,
    artifact_kind,
    canonical_json,
    extract_source_identity,
    file_sha256,
    product_version,
    validate_source_identity,
    verify_reproducible_artifact,
    verify_release_sources,
)


ROOT = Path(__file__).resolve().parents[2]
MAX_ARTIFACT_BYTES = 2 * 1024 * 1024 * 1024


def fail(message: str) -> None:
    raise SystemExit(f"release manifest verification error: {message}")


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON object key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> tuple[dict, bytes]:
    try:
        payload = path.read_bytes()
        value = json.loads(
            payload.decode("utf-8"),
            object_pairs_hook=unique_object,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path} must contain a JSON object")
    return value, payload


def validate_digest(value: object, description: str) -> None:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        fail(f"{description} is malformed")


def validate_structure(manifest: dict) -> tuple[dict, set[str]]:
    if set(manifest) != {
        "schemaVersion",
        "productVersion",
        "target",
        "source",
        "pins",
        "artifacts",
    }:
        fail("manifest top-level fields differ from schema 6")
    if manifest["schemaVersion"] != 6:
        fail("unsupported manifest schema")
    if not isinstance(manifest["productVersion"], str) or not re.fullmatch(
        r"[0-9]+\.[0-9]+\.[0-9]+",
        manifest["productVersion"],
    ):
        fail("manifest product version is malformed")
    if manifest["target"] != "linux-x86_64":
        fail("manifest release target is unsupported")
    try:
        source = validate_source_identity(manifest["source"])
    except ValueError as error:
        fail(str(error))
    if source["dirty"]:
        fail("release manifest records dirty source")

    pins = manifest["pins"]
    if not isinstance(pins, dict) or set(pins) != {
        "qtVersion",
        "runtime",
        "ghostty",
        "dependenciesLockSha256",
    }:
        fail("manifest pins are malformed")
    runtime = pins["runtime"]
    ghostty = pins["ghostty"]
    if (
        not isinstance(pins["qtVersion"], str)
        or not pins["qtVersion"]
        or not isinstance(runtime, dict)
        or set(runtime)
        != {
            "ffiAbiVersion",
            "desktopProtocolVersion",
        }
        or not isinstance(ghostty, dict)
        or set(ghostty)
        != {
            "repository",
            "commit",
            "linuxVtArchiveSha256",
        }
    ):
        fail("manifest pins are malformed")
    if not isinstance(ghostty["repository"], str) or not re.fullmatch(
        r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", ghostty["repository"],
    ):
        fail("manifest Ghostty repository is malformed")
    if not isinstance(ghostty["commit"], str) or not re.fullmatch(
        r"[0-9a-f]{40}", ghostty["commit"],
    ):
        fail("manifest Ghostty commit is malformed")
    if (
        not isinstance(runtime["ffiAbiVersion"], int)
        or runtime["ffiAbiVersion"] <= 0
        or not isinstance(runtime["desktopProtocolVersion"], int)
        or runtime["desktopProtocolVersion"] <= 0
    ):
        fail("manifest runtime versions are malformed")
    validate_digest(
        ghostty["linuxVtArchiveSha256"],
        "Ghostty archive digest",
    )
    validate_digest(pins["dependenciesLockSha256"], "dependency lock digest")
    artifact_rows = manifest["artifacts"]
    if not isinstance(artifact_rows, list) or len(artifact_rows) != 3:
        fail("manifest must contain exactly three artifacts")
    names: set[str] = set()
    kinds: list[str] = []
    ordered_names: list[str] = []
    for row in artifact_rows:
        if (
            not isinstance(row, dict)
            or set(row)
            != {"filename", "size", "sha256", "sourceIdentity"}
        ):
            fail("artifact row is malformed")
        name = row["filename"]
        if (
            not isinstance(name, str)
            or Path(name).name != name
            or "/" in name
            or "\\" in name
            or name in {".", ".."}
        ):
            fail("artifact filename contains path traversal")
        if name in names:
            fail("artifact names must be unique")
        names.add(name)
        ordered_names.append(name)
        try:
            kinds.append(artifact_kind(name))
        except ValueError as error:
            fail(str(error))
        if (
            not isinstance(row["size"], int)
            or isinstance(row["size"], bool)
            or row["size"] <= 0
            or row["size"] > MAX_ARTIFACT_BYTES
        ):
            fail(f"artifact size is malformed: {name}")
        validate_digest(row["sha256"], f"artifact digest for {name}")
        try:
            recorded = validate_source_identity(row["sourceIdentity"])
        except ValueError as error:
            fail(str(error))
        if recorded != source:
            fail(f"artifact source identities differ: {name}")
    if sorted(kinds) != ["arch", "deb", "tgz"]:
        fail("manifest requires exactly one DEB, one TGZ, and one Arch artifact")
    if ordered_names != sorted(ordered_names):
        fail("manifest artifact rows are not deterministically ordered")
    return source, names


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--structure-only", action="store_true")
    parser.add_argument(
        "--dependencies",
        type=Path,
        default=ROOT / "dependencies.lock.json",
    )
    arguments = parser.parse_args()

    if arguments.manifest.is_symlink():
        fail("manifest must not be a symlink")
    manifest_path = arguments.manifest.resolve(strict=True)
    if manifest_path.name != "release-manifest.json":
        fail("manifest must have the exact filename release-manifest.json")
    manifest, payload = load_json(manifest_path)
    source, names = validate_structure(manifest)
    if payload != canonical_json(manifest):
        fail("manifest is not canonical JSON")
    if arguments.structure_only:
        print("release manifest structure verified offline")
        return
    if arguments.output_dir is None:
        fail("--output-dir is required for artifact verification")
    output_dir = arguments.output_dir.resolve(strict=True)
    if manifest_path.parent != output_dir:
        fail("manifest must be in output-dir")

    try:
        dependencies, _ = load_json(arguments.dependencies)
        measured_source, measured_ghostty = verify_release_sources(
            arguments.source_root,
            require_clean=True,
        )
        expected_version = product_version(arguments.source_root)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        fail(str(error))
    if source != measured_source:
        fail("manifest source identities differ from current checkouts")
    if manifest["productVersion"] != expected_version:
        fail("manifest product version differs from source")

    runtime = dependencies["kodosi"]
    ghostty = dependencies["ghostty"]
    expected_pins = {
        "qtVersion": dependencies["frameworks"]["qt"]["version"],
        "runtime": {
            "ffiAbiVersion": runtime["ffiAbiVersion"],
            "desktopProtocolVersion": runtime["desktopProtocolVersion"],
        },
        "ghostty": {
            "repository": ghostty["repository"],
            "commit": measured_ghostty["linuxUpstreamCommit"],
            "linuxVtArchiveSha256": measured_ghostty[
                "linuxVtArchiveSha256"
            ],
        },
        "dependenciesLockSha256": file_sha256(arguments.dependencies),
    }
    if manifest["pins"] != expected_pins:
        fail("manifest pins differ from dependencies.lock.json")
    for row in manifest["artifacts"]:
        name = row["filename"]
        candidate = output_dir / name
        if candidate.is_symlink():
            fail(f"artifact is a symlink: {name}")
        try:
            resolved = candidate.resolve(strict=True)
        except OSError as error:
            fail(f"artifact cannot be resolved: {error}")
        if resolved.parent != output_dir or not resolved.is_file():
            fail(f"artifact escaped output-dir or is not regular: {name}")
        size = resolved.stat().st_size
        if row["size"] != size:
            fail(f"artifact size mismatch: {name}")
        if row["sha256"] != file_sha256(resolved):
            fail(f"artifact digest mismatch: {name}")
        try:
            verify_reproducible_artifact(
                resolved,
                measured_source["sourceDateEpoch"],
            )
        except ValueError as error:
            fail(str(error))
        try:
            embedded = extract_source_identity(resolved)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            fail(str(error))
        if embedded != source:
            fail(f"artifact embedded source identities differ: {name}")

    package_files = {
        path.name
        for path in output_dir.iterdir()
        if path.is_file()
        and any(path.name.endswith(suffix) for suffix in ARTIFACT_SUFFIXES)
    }
    if package_files != names:
        fail("output-dir package set differs from the canonical manifest")
    print("release manifest verified: exact DEB/TGZ/Arch set")


if __name__ == "__main__":
    main()
