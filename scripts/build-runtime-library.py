#!/usr/bin/env python3
"""Builds the runtime library, which is the static library of the crate kodosi-ffi-c,
and copies it, with its generated C header, to <output-dir>/<target>/<profile>. Prints
that folder."""

from __future__ import annotations

import argparse
import filecmp
import json
import shutil
import subprocess
import sys
from pathlib import Path


def archive_name(target: str) -> str:
    return "kodosi_ffi_c.lib" if target.endswith("-windows-msvc") else "libkodosi_ffi_c.a"


def build(manifest: Path, output: Path, profile: str, target: str, default_features: bool) -> Path:
    bundle = output.resolve() / target / profile
    # cl.exe cannot write a path longer than 259 characters, and Cargo repeats the target
    # and the profile below its own folder. Windows builds share one Cargo folder.
    cargo = output.resolve() / "cargo" if target.endswith("-windows-msvc") else bundle / "cargo"
    command = ["cargo", "build", "--locked", "--manifest-path", str(manifest),
        "--target-dir", str(cargo), "--target", target, "--profile", profile,
        "-p", "kodosi-ffi-c", "--message-format=json-render-diagnostics"]
    if not default_features:
        command.append("--no-default-features")
    # Cargo prints the compiler messages itself. The JSON lines on stdout name the
    # archive, and the build script folder that holds the header.
    process = subprocess.Popen(command, cwd=manifest.parent, stdout=subprocess.PIPE, text=True)
    assert process.stdout is not None
    archives: dict[str, Path] = {}
    out_dirs: dict[str, Path] = {}
    for line in process.stdout:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            print(line, file=sys.stderr, end="")
            continue
        if message.get("reason") == "compiler-artifact":
            for name in message["filenames"]:
                if Path(name).name == archive_name(target):
                    archives[message["package_id"]] = Path(name)
        elif message.get("reason") == "build-script-executed":
            out_dirs[message["package_id"]] = Path(message["out_dir"])
    if process.wait() != 0:
        raise SystemExit("Cargo could not build the runtime library")
    if len(archives) != 1:
        raise SystemExit("Cargo did not report one runtime library")
    [(package, archive)] = archives.items()
    header = out_dirs[package] / "kodosi_runtime.h"
    for source, destination in ((archive, bundle / archive_name(target)),
                                (header, bundle / "include/kodosi_runtime.h")):
        destination.parent.mkdir(parents=True, exist_ok=True)
        # An unchanged file keeps its time, so the app does not link again.
        if not destination.exists() or not filecmp.cmp(source, destination, shallow=False):
            shutil.copyfile(source, destination)
    return bundle


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--no-default-features", action="store_true")
    arguments = parser.parse_args()
    print(build(arguments.manifest.resolve(), arguments.output_dir, arguments.profile,
        arguments.target, not arguments.no_default_features))


if __name__ == "__main__":
    main()
