#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

from release_common import (
    ARTIFACT_SUFFIXES,
    artifact_kind,
    canonical_json,
    extract_source_identity,
    file_sha256,
    product_version,
    verify_reproducible_artifact,
    verify_release_sources,
)


ROOT = Path(__file__).resolve().parents[2]
MAX_ARTIFACT_BYTES = 2 * 1024 * 1024 * 1024


def fail(message: str) -> None:
    raise SystemExit(f"release manifest error: {message}")


def load_object(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path} must contain a JSON object")
    return value


def validate_artifact(output_dir: Path, argument: str) -> Path:
    candidate = Path(argument)
    if not candidate.is_absolute():
        candidate = output_dir / candidate
    if candidate.is_symlink():
        fail(f"artifact is a symlink: {candidate.name}")
    try:
        resolved = candidate.resolve(strict=True)
    except OSError as error:
        fail(f"artifact cannot be resolved: {error}")
    if resolved.parent != output_dir or not resolved.is_file():
        fail(f"artifact is outside output-dir or not regular: {candidate}")
    if not any(resolved.name.endswith(suffix) for suffix in ARTIFACT_SUFFIXES):
        fail(f"unexpected artifact type: {resolved.name}")
    size = resolved.stat().st_size
    if size <= 0 or size > MAX_ARTIFACT_BYTES:
        fail(f"artifact size is outside the allowed bounds: {resolved.name}")
    return resolved


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--output", default="release-manifest.json")
    parser.add_argument("--artifact", action="append", required=True)
    parser.add_argument(
        "--dependencies",
        type=Path,
        default=ROOT / "dependencies.lock.json",
    )
    arguments = parser.parse_args()

    dependencies = load_object(arguments.dependencies)
    try:
        measured_source, measured_ghostty = verify_release_sources(
            arguments.source_root,
            require_clean=True,
        )
        version = product_version(arguments.source_root)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        fail(str(error))

    output_dir = arguments.output_dir.resolve(strict=True)
    output = Path(arguments.output)
    if output.name != str(output) or output.name != "release-manifest.json":
        fail("output must be the exact filename release-manifest.json")
    output_path = output_dir / output
    if output_path.is_symlink():
        fail("manifest output must not be a symlink")

    artifacts = [
        validate_artifact(output_dir, argument)
        for argument in arguments.artifact
    ]
    names = [artifact.name for artifact in artifacts]
    if len(names) != len(set(names)):
        fail("artifact names must be unique")
    kinds = [artifact_kind(name) for name in names]
    if sorted(kinds) != ["arch", "deb", "tgz"]:
        fail("release requires exactly one DEB, one TGZ, and one Arch artifact")

    artifact_identities: dict[str, dict[str, object]] = {}
    for artifact in artifacts:
        try:
            verify_reproducible_artifact(
                artifact,
                measured_source["sourceDateEpoch"],
            )
        except ValueError as error:
            fail(str(error))
        try:
            identity = extract_source_identity(artifact)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            fail(str(error))
        if identity != measured_source:
            fail(f"artifact source identities differ from checkouts: {artifact.name}")
        artifact_identities[artifact.name] = identity

    if dependencies.get("schemaVersion") != 1:
        fail("unsupported dependency lock schema")
    runtime = dependencies["kodosi"]
    ghostty = dependencies["ghostty"]

    manifest = {
        "schemaVersion": 6,
        "productVersion": version,
        "target": "linux-x86_64",
        "source": measured_source,
        "pins": {
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
        },
        "artifacts": [
            {
                "filename": artifact.name,
                "size": artifact.stat().st_size,
                "sha256": file_sha256(artifact),
                "sourceIdentity": artifact_identities[artifact.name],
            }
            for artifact in sorted(artifacts, key=lambda value: value.name)
        ],
    }
    encoded = canonical_json(manifest)
    temporary = output_dir / ".release-manifest.json.part"
    if temporary.is_symlink():
        fail("temporary manifest path must not be a symlink")
    try:
        with temporary.open("wb") as destination:
            destination.write(encoded)
            destination.flush()
            os.fsync(destination.fileno())
        os.replace(temporary, output_path)
    finally:
        temporary.unlink(missing_ok=True)
    print(output_path)


if __name__ == "__main__":
    main()
