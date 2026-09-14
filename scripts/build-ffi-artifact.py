#!/usr/bin/env python3

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def resolve_outputs(messages: list[dict], crate_manifest: Path, target_dir: Path) -> tuple[Path, Path]:
    candidates = [message for message in messages
        if message.get("reason") == "compiler-artifact"
        and Path(message.get("manifest_path", "")).resolve() == crate_manifest
        and "staticlib" in message.get("target", {}).get("crate_types", [])]
    if len(candidates) != 1:
        raise ValueError("Cargo did not report exactly one requested FFI static library")
    artifact = candidates[0]
    archives = [Path(name) for name in artifact.get("filenames", []) if name.endswith(".a")]
    outputs = [Path(message["out_dir"]) for message in messages
        if message.get("reason") == "build-script-executed"
        and message.get("package_id") == artifact.get("package_id")]
    if len(archives) != 1 or len(outputs) != 1:
        raise ValueError("Cargo did not bind one generated header to the FFI archive")
    archive, header = archives[0], outputs[0] / "kodosi_runtime.h"
    for path in (archive, header):
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(target_dir.resolve()):
            raise ValueError(f"Cargo artifact is missing or outside its private target directory: {path}")
    return archive.resolve(), header.resolve()


def publish(slot: Path, archive: Path, header: Path, target: str, profile: str) -> Path:
    record = {
        "schemaVersion": 1,
        "target": target,
        "profile": profile,
        "archiveSha256": digest(archive),
        "headerSha256": digest(header),
    }
    identity = hashlib.sha256(canonical_json(record)).hexdigest()
    bundles = slot / "bundles"
    bundles.mkdir(exist_ok=True)
    bundle = bundles / identity
    if not bundle.exists():
        with tempfile.TemporaryDirectory(prefix=".publish-", dir=slot) as temporary:
            stage = Path(temporary) / "artifact"
            (stage / "include").mkdir(parents=True)
            shutil.copyfile(archive, stage / "libkodosi_ffi_c.a")
            shutil.copyfile(header, stage / "include/kodosi_runtime.h")
            if digest(stage / "libkodosi_ffi_c.a") != record["archiveSha256"] or digest(stage / "include/kodosi_runtime.h") != record["headerSha256"]:
                raise ValueError("FFI artifacts changed while publication was being prepared")
            (stage / "artifact.json").write_bytes(canonical_json(record))
            for path in stage.rglob("*"):
                if path.is_file():
                    with path.open("rb") as file:
                        os.fsync(file.fileno())
            stage.rename(bundle)
    if (bundle / "artifact.json").read_bytes() != canonical_json(record) or digest(bundle / "libkodosi_ffi_c.a") != record["archiveSha256"] or digest(bundle / "include/kodosi_runtime.h") != record["headerSha256"]:
        raise ValueError("An existing FFI artifact bundle does not match its content identity")
    link = slot / f".current-{os.getpid()}"
    try:
        link.symlink_to(Path("bundles") / identity, target_is_directory=True)
        os.replace(link, slot / "current")
    finally:
        link.unlink(missing_ok=True)
    return bundle


def consume_and_prune(slot: Path, bundle: Path, consumer: list[str] | None) -> None:
    try:
        if consumer:
            subprocess.run([*consumer, str(bundle)], check=True)
    finally:
        for previous in (slot / "bundles").iterdir():
            if previous != bundle:
                shutil.rmtree(previous)


def build(manifest: Path, output: Path, profile: str, target: str, offline: bool,
          consumer: list[str] | None = None) -> Path:
    manifest = manifest.resolve(strict=True)
    crate_manifest = (manifest.parent / "crates/kodosi-ffi-c/Cargo.toml").resolve(strict=True)
    if target in {".", ".."} or not re.fullmatch(r"[A-Za-z0-9_.-]+", target) or not re.fullmatch(r"[A-Za-z0-9_-]+", profile):
        raise ValueError("FFI target and profile must be explicit safe identifiers")
    slot = output.resolve() / target / profile
    slot.mkdir(parents=True, exist_ok=True)
    with (slot / ".build.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        owner = slot / "source-manifest"
        identity = str(manifest) + "\n"
        if owner.exists() and owner.read_text() != identity:
            raise ValueError("FFI output directory belongs to another source manifest")
        if not owner.exists():
            owner.write_text(identity)
        target_dir = slot / "cargo"
        package_id = subprocess.check_output(
            ["cargo", "pkgid", "--locked", "--manifest-path", str(crate_manifest)],
            cwd=manifest.parent, text=True).strip()
        command = ["cargo", "build", "--locked", "--manifest-path", str(manifest),
            "--target-dir", str(target_dir), "--target", target, "--profile", profile,
            "-p", "kodosi-ffi-c", "--message-format=json-render-diagnostics"]
        if offline:
            command.append("--offline")
        process = subprocess.Popen(command, cwd=manifest.parent, stdout=subprocess.PIPE, text=True)
        assert process.stdout is not None
        messages = []
        for line in process.stdout:
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                print(line, file=sys.stderr, end="")
                continue
            if message.get("reason") == "compiler-message":
                rendered = message.get("message", {}).get("rendered")
                if rendered:
                    print(rendered, file=sys.stderr, end="")
            elif message.get("reason") == "build-finished" or (
                message.get("reason") == "compiler-artifact"
                and Path(message.get("manifest_path", "")).resolve() == crate_manifest
                and "staticlib" in message.get("target", {}).get("crate_types", [])
            ) or (
                message.get("reason") == "build-script-executed"
                and message.get("package_id") == package_id
            ):
                messages.append(message)
        if process.wait() != 0:
            raise ValueError("Cargo FFI build failed; previous published bundle was left unchanged")
        if not any(message.get("reason") == "build-finished" and message.get("success") is True for message in messages):
            raise ValueError("Cargo did not confirm successful artifact completion")
        archive, header = resolve_outputs(messages, crate_manifest, target_dir)
        bundle = publish(slot, archive, header, target, profile)
        consume_and_prune(slot, bundle, consumer)
        return slot / "current"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--consume-command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    try:
        bundle = build(arguments.manifest, arguments.output_dir, arguments.profile, arguments.target,
            arguments.offline, arguments.consume_command)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"FFI artifact publication failed: {error}") from error
    print(bundle)


if __name__ == "__main__":
    main()
