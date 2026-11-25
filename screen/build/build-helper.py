#!/usr/bin/env python3
"""Builds "Kodosi Screen", the app that captures this Mac's screens and windows for the host, into
<output-dir> and prints its path. The Mac app's build puts it in Contents/Helpers and signs it. It
stays an app of its own, so that macOS asks for Screen Recording and Accessibility in its name and
agents in Kodosi's terminals do not inherit them. With --notices <dir>, also writes the licenses of
its Rust crates to <dir>."""

# Xcode runs this with its own Python 3.9.
from __future__ import annotations

import argparse
import os
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKSPACE = ROOT / "screen"
APP = "Kodosi Screen"
APP_ID = "com.kodosi.screen"
EXECUTABLE = "kodosi-screen"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--target", default="aarch64-apple-darwin")
    parser.add_argument("--icon", type=Path, help="the .icns file of the app that it ships in")
    parser.add_argument("--notices", type=Path)
    parser.add_argument("--debug", action="store_true", help="a quick build for development")
    arguments = parser.parse_args()
    profile = "debug" if arguments.debug else "release"
    minimum = (ROOT / "clients/macos/Config/macOSDeploymentTarget").read_text(encoding="utf-8").strip()
    command = [
        "cargo", "build", "--locked", "--quiet",
        "--manifest-path", str(WORKSPACE / "Cargo.toml"),
        "--package", "screen-helper",
        "--target", arguments.target,
    ]
    if not arguments.debug:
        command.append("--release")
    # Run from ROOT so rustup picks up the pinned toolchain in rust-toolchain.toml.
    environment = {**os.environ, "CARGO_INCREMENTAL": "0", "MACOSX_DEPLOYMENT_TARGET": minimum}
    subprocess.run(command, cwd=ROOT, check=True, env=environment)

    app = arguments.output_dir.resolve() / f"{APP}.app"
    shutil.rmtree(app, ignore_errors=True)
    (app / "Contents/MacOS").mkdir(parents=True)
    shutil.copy2(WORKSPACE / "target" / arguments.target / profile / EXECUTABLE, app / "Contents/MacOS" / EXECUTABLE)
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
    if arguments.notices:
        shutil.rmtree(arguments.notices, ignore_errors=True)
        subprocess.run(
            [sys.executable, str(ROOT / "clients/qt/scripts/release/generate-rust-license-inventory.py"),
             "--manifest", str(WORKSPACE / "Cargo.toml"), "--target", arguments.target,
             "--root", "screen-helper",
             "--overrides", str(ROOT / "clients/qt/packaging/licenses/screen-license-overrides.json"),
             "--output-dir", str(arguments.notices)],
            cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    print(app)


if __name__ == "__main__":
    main()
