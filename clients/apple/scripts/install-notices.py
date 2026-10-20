#!/usr/bin/env python3
"""Installs the notices of the Mac app or of the iOS app in the resources folder of the
app. First it checks the Ghostty notices with verify-third-party-notices.py. Builds other
than Debug also get the licenses of the Rust crates in the runtime, which the iOS app gets
as one file for each crate."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath

IOS_SLICES = {"iphoneos": "ios-arm64", "iphonesimulator": "ios-arm64-simulator"}
RUST_TARGETS = {
    "macosx": "aarch64-apple-darwin",
    "iphoneos": "aarch64-apple-ios",
    "iphonesimulator": "aarch64-apple-ios-sim",
}
MAC_GHOSTTY_FILES = (
    "LICENSE",
    "LICENSE-GHOSTTY",
    "THIRD_PARTY_NOTICES.md",
    "scripts/relink-libintl.sh",
    "Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a",
)
SOURCE_ARCHIVES = PurePosixPath("ThirdPartyNotices/corresponding-source")


def repository_root() -> Path:
    return Path(__file__).resolve().parents[3]


def absent_files(ghostty: Path, slice_name: str) -> set[str]:
    inventory = json.loads((ghostty / "ThirdPartyNotices/inventory.json").read_text(encoding="utf-8"))
    absent = set(inventory["appleSlices"][slice_name]["absentComponents"])
    names: dict[bool, set[str]] = {True: set(), False: set()}
    for component in inventory["components"]:
        files = names[component["id"] in absent]
        files.update(component["notices"])
        files.update(
            PurePosixPath(path).name
            for path in component["sourcePaths"]
            if PurePosixPath(path).parent == SOURCE_ARCHIVES
        )
    return names[True] - names[False]


def notice_files(platform: str, ghostty: Path) -> dict[str, Path]:
    artwork = sorted((repository_root() / "clients/apple/Notices").glob("*.txt"))
    if platform == "macosx":
        mac = sorted((repository_root() / "clients/macos/Notices").glob("*.txt"))
        files = {source.name: source for source in [*artwork, *mac]}
        files.update({f"GhosttyNativeNotices/{name}": ghostty / name for name in MAC_GHOSTTY_FILES})
        for source in sorted((ghostty / "ThirdPartyNotices").rglob("*")):
            if source.is_file():
                files[f"GhosttyNativeNotices/{source.relative_to(ghostty).as_posix()}"] = source
        return files
    absent = absent_files(ghostty, IOS_SLICES[platform])
    files = {f"Notices/{source.name}": source for source in artwork}
    files["Notices/Ghostty/libghostty-spm.txt"] = ghostty / "LICENSE"
    files["Notices/Ghostty/Ghostty.txt"] = ghostty / "LICENSE-GHOSTTY"
    files["Notices/Ghostty/Third-party notices.md"] = ghostty / "THIRD_PARTY_NOTICES.md"
    files["GhosttySource/inventory.json"] = ghostty / "ThirdPartyNotices/inventory.json"
    for source in sorted((ghostty / "ThirdPartyNotices/licenses").glob("*.txt")):
        if source.name not in absent:
            files[f"Notices/Ghostty/Licenses/{source.name}"] = source
    for source in sorted(ghostty.joinpath(*SOURCE_ARCHIVES.parts).glob("*.tar*")):
        if source.name not in absent:
            files[f"GhosttySource/{source.name}"] = source
    return files


def rust_notices(platform: str, output: Path) -> dict[str, Path]:
    root = repository_root()
    inventory = [
        sys.executable,
        str(root / "clients/qt/scripts/release/generate-rust-license-inventory.py"),
        "--manifest",
        str(root / "runtime/Cargo.toml"),
        "--target",
        RUST_TARGETS[platform],
        "--output-dir",
        str(output),
    ]
    # Xcode gives its build phases a PATH without the Rust tools.
    path = f"{Path.home() / '.cargo/bin'}{os.pathsep}{os.environ.get('PATH', '')}"
    if subprocess.run(inventory, stdout=subprocess.DEVNULL, env={**os.environ, "PATH": path}).returncode != 0:
        raise SystemExit("[!] notices: the licenses of the Rust crates could not be collected")
    if platform != "macosx":
        return {f"Notices/Rust/{source.name}": source for source in crate_notices(output)}
    return {
        f"RustNotices/{source.relative_to(output).as_posix()}": source
        for source in sorted(output.rglob("*"))
        if source.is_file()
    }


def crate_notices(output: Path) -> list[Path]:
    inventory = json.loads((output / "rust-dependency-licenses.json").read_text(encoding="utf-8"))
    folder = output / "crates"
    folder.mkdir()
    files = []
    for crate in inventory["dependencies"]:
        notice = folder / f"{crate['name']} {crate['version']}.txt"
        if notice.exists():
            raise SystemExit(f"[!] notices: two Rust crates are named {notice.stem}")
        parts = [crate["license"]]
        for evidence in crate["licenseFiles"]:
            parts.append("/".join(Path(evidence["path"]).parts[2:]))
            parts.append((output / evidence["path"]).read_text(encoding="utf-8", errors="replace").strip())
        notice.write_text("\n\n".join(parts) + "\n", encoding="utf-8")
        files.append(notice)
    return files


def install(platform: str, configuration: str, resources: Path) -> None:
    ghostty = Path(os.environ.get("KODOSI_GHOSTTY_DIR") or repository_root() / "terminal/ghostty")
    check = [sys.executable, str(ghostty / "scripts/verify-third-party-notices.py")]
    if os.environ.get("KODOSI_WORKING_TREE_VALIDATION") == "1":
        check.append("--working-tree")
    if subprocess.run(check, stdout=subprocess.DEVNULL).returncode != 0:
        raise SystemExit("[!] notices: the Ghostty notices differ from terminal/ghostty/ThirdPartyNotices/inventory.json")
    files = notice_files(platform, ghostty)
    resources.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kodosi-rust-notices-") as rust:
        if configuration != "Debug":
            files.update(rust_notices(platform, Path(rust)))
        with tempfile.TemporaryDirectory(prefix=".notices-", dir=resources) as temporary:
            staging = Path(temporary)
            for name, source in files.items():
                (staging / name).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, staging / name)
                shutil.copymode(source, staging / name)
            for entry in sorted(staging.iterdir()):
                destination = resources / entry.name
                if destination.is_dir() and not destination.is_symlink():
                    shutil.rmtree(destination)
                os.replace(entry, destination)
    print(f"[*] Installed {len(files)} notice files for {platform} in {resources}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("macosx", *IOS_SLICES))
    parser.add_argument("configuration", help="the Xcode build configuration")
    parser.add_argument("resources", type=Path, help="the resources folder of the app")
    arguments = parser.parse_args()
    install(arguments.platform, arguments.configuration, arguments.resources)


if __name__ == "__main__":
    main()
