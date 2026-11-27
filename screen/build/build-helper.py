#!/usr/bin/env python3
"""Builds "Kodosi Screen", which captures this computer's screens for the host, into <output-dir> and
prints its path. For a Mac it is an app, which the Mac app's build puts in Contents/Helpers and
signs. It stays an app of its own, so that macOS asks for Screen Recording and Accessibility in its
name and agents in Kodosi's terminals do not inherit them. For Linux and Windows it is a program,
which the Linux and Windows app puts next to the kodosi program. With --notices <dir>, also writes
the licenses of its Rust crates to <dir>."""

# Xcode runs this with its own Python 3.9.
from __future__ import annotations

import argparse
import filecmp
import os
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKSPACE = ROOT / "screen"
APP = "Kodosi"
APP_ID = "com.kodosi.screen"
EXECUTABLE = "kodosi-screen"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--target", default="aarch64-apple-darwin")
    parser.add_argument("--icon", type=Path, help="the .icns file of the Mac app that it ships in")
    parser.add_argument("--notices", type=Path)
    parser.add_argument("--debug", action="store_true", help="a quick build for development")
    arguments = parser.parse_args()
    mac = arguments.target.endswith("-apple-darwin")
    profile = "debug" if arguments.debug else "release"
    command = [
        "cargo", "build", "--locked", "--quiet",
        "--manifest-path", str(WORKSPACE / "Cargo.toml"),
        "--target-dir", str(WORKSPACE / "target"),
        "--package", "screen-helper",
        "--target", arguments.target,
    ]
    if not arguments.debug:
        command.append("--release")
    # Run from ROOT so rustup picks up the pinned toolchain in rust-toolchain.toml.
    environment = {**os.environ, "CARGO_INCREMENTAL": "0"}
    if mac:
        minimum = (ROOT / "clients/macos/Config/macOSDeploymentTarget").read_text(encoding="utf-8").strip()
        environment["MACOSX_DEPLOYMENT_TARGET"] = minimum
    subprocess.run(command, cwd=ROOT, check=True, env=environment)
    name = EXECUTABLE + (".exe" if arguments.target.endswith("-windows-msvc") else "")
    built = WORKSPACE / "target" / arguments.target / profile / name
    if not mac:
        program = arguments.output_dir.resolve() / name
        program.parent.mkdir(parents=True, exist_ok=True)
        if not program.is_file() or not filecmp.cmp(built, program, shallow=False):
            shutil.copy2(built, program)
        write_notices(arguments.target, arguments.notices)
        print(program)
        return

    app = arguments.output_dir.resolve() / f"{APP}.app"
    shutil.rmtree(app, ignore_errors=True)
    (app / "Contents/MacOS").mkdir(parents=True)
    shutil.copy2(built, app / "Contents/MacOS" / EXECUTABLE)
    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    info = {
        "CFBundleDisplayName": APP,
        "CFBundleExecutable": EXECUTABLE,
        "CFBundleIdentifier": APP_ID,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": APP,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": minimum,
        "LSUIElement": True,
    }
    if arguments.icon:
        (app / "Contents/Resources").mkdir()
        shutil.copy2(arguments.icon, app / "Contents/Resources" / arguments.icon.name)
        info["CFBundleIconFile"] = arguments.icon.stem
    with (app / "Contents/Info.plist").open("wb") as file:
        plistlib.dump(info, file)
    write_notices(arguments.target, arguments.notices)
    print(app)


def write_notices(target: str, notices: Path | None) -> None:
    if not notices:
        return
    shutil.rmtree(notices, ignore_errors=True)
    subprocess.run(
        [sys.executable, str(ROOT / "clients/qt/scripts/release/generate-rust-license-inventory.py"),
         "--manifest", str(WORKSPACE / "Cargo.toml"), "--target", target,
         "--root", "screen-helper",
         "--overrides", str(ROOT / "clients/qt/packaging/licenses/screen-license-overrides.json"),
         "--output-dir", str(notices)],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


if __name__ == "__main__":
    main()
