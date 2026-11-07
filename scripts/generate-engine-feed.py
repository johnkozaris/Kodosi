#!/usr/bin/env python3
"""Writes engine.json, the signed feed of Kodosi Browser, and engine.json.sig in a folder with the
packages that browser/build/build-engine.py made for each platform. The feed names each package
by its download in the GitHub release browser-<version>. Sparkle's sign_update signs the feed
with the private updates key, which comes on standard input."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "johnkozaris/Kodosi"
PLATFORMS = ("mac-arm64", "mac-x64", "linux-x64", "linux-arm64", "windows-x64")


def fail(message: str) -> None:
    raise SystemExit(f"generate-engine-feed: {message}")


def feed(folder: Path) -> dict:
    entries = [json.loads(path.read_text("utf-8")) for path in sorted(folder.glob("kodosi-browser-*.tar.gz.json"))]
    if not entries:
        fail(f"{folder} has no packages of Kodosi Browser")
    versions = sorted({entry["version"] for entry in entries})
    if len(versions) != 1:
        fail(f"the packages have more than one version: {', '.join(versions)}")
    version = versions[0]
    engines = {}
    for entry in entries:
        platform, archive = entry["platform"], folder / entry["file"]
        if platform not in PLATFORMS:
            fail(f"{archive.name} is for {platform}, which Kodosi does not know")
        if platform in engines:
            fail(f"two packages are for {platform}")
        if not archive.is_file() or hashlib.sha256(archive.read_bytes()).hexdigest() != entry["sha256"]:
            fail(f"{archive.name} is missing or does not match its entry")
        engines[platform] = {
            "url": f"https://github.com/{REPOSITORY}/releases/download/browser-{version}/{archive.name}",
            "sha256": entry["sha256"],
            "program": entry["program"],
        }
    return {"version": version, "engines": engines}


def sign_update() -> Path:
    project = ROOT / "clients/macos/KodosiDesktop.xcodeproj"
    if not (project / "project.pbxproj").exists():
        subprocess.run(["just", "--justfile", str(ROOT / "clients/macos/justfile"), "xcode-gen"], stdin=subprocess.DEVNULL, check=True)
    packages = ROOT / "clients/macos/build/packages"
    subprocess.run(
        ["xcodebuild", "-resolvePackageDependencies", "-onlyUsePackageVersionsFromResolvedFile", "-project", str(project),
         "-scheme", "KodosiDesktop", "-clonedSourcePackagesDirPath", str(packages)],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, check=True,
    )
    return packages / "artifacts/sparkle/Sparkle/bin/sign_update"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path, help="the folder with the packages and their .json entries")
    folder = parser.parse_args().folder
    if sys.stdin.isatty():
        fail("give the private updates key on standard input")
    path = folder / "engine.json"
    path.write_text(json.dumps(feed(folder), indent=2) + "\n", "utf-8")
    signature = subprocess.run(
        [str(sign_update()), "--ed-key-file", "-", "-p", "--disable-signing-warning", str(path)],
        stdin=sys.stdin, capture_output=True, text=True, check=True,
    ).stdout.strip()
    (folder / "engine.json.sig").write_text(signature + "\n", "utf-8")
    print(f"[*] Wrote the feed of Kodosi Browser in {path}")


if __name__ == "__main__":
    main()
