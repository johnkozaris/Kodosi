#!/usr/bin/env python3
"""Builds the agent-browser that AgentBrowser.lock pins, from its Git commit, with the pinned
Rust and its own Cargo.lock, and copies it to <output-dir>. With --notices, writes its
license and the licenses of its Rust crates to that folder. Prints the path of the program."""

from __future__ import annotations

import argparse
import filecmp
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "https://github.com/vercel-labs/agent-browser.git"
WORK = ROOT / ".tools/agent-browser"


def pin() -> dict[str, str]:
    values = {}
    for line in (ROOT / "AgentBrowser.lock").read_text(encoding="utf-8").splitlines():
        name, _, value = line.partition("=")
        values[name.strip()] = value.strip()
    commit = values.get("commit", "")
    if set(values) != {"version", "commit"} or len(commit) != 40 or not all(c in "0123456789abcdef" for c in commit):
        raise SystemExit("AgentBrowser.lock names its version and the commit of that version")
    return values


def git(folder: Path, *arguments: str) -> str:
    result = subprocess.run(["git", *arguments], cwd=folder, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def source(commit: str, version: str) -> Path:
    folder = WORK / commit
    if not (folder / "cli/Cargo.toml").is_file():
        WORK.mkdir(parents=True, exist_ok=True)
        fetched = Path(tempfile.mkdtemp(prefix=".fetch-", dir=WORK))
        try:
            git(fetched, "init", "--quiet")
            git(fetched, "fetch", "--quiet", "--depth", "1", REPOSITORY, commit)
            git(fetched, "-c", "advice.detachedHead=false", "checkout", "--quiet", "FETCH_HEAD")
            os.replace(fetched, folder)
        finally:
            shutil.rmtree(fetched, ignore_errors=True)
    if git(folder, "rev-parse", "HEAD") != commit or git(folder, "status", "--porcelain", "--untracked-files=no"):
        raise SystemExit(f"{folder} is not commit {commit}. Delete it and build again.")
    manifest = (folder / "cli/Cargo.toml").read_text(encoding="utf-8")
    if f'version = "{version}"' not in manifest:
        raise SystemExit(f"Commit {commit} is not agent-browser {version}")
    return folder


def build(folder: Path, target: str) -> Path:
    environment = {
        **os.environ,
        "CARGO_ENCODED_RUSTFLAGS": f"--remap-path-prefix={Path.home()}=~",
        "CARGO_INCREMENTAL": "0",
    }
    # Cargo takes the pinned Rust of rust-toolchain.toml from the folder that it runs in.
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "--quiet",
         "--manifest-path", str(folder / "cli/Cargo.toml"),
         "--target", target, "--target-dir", str(folder / "target")],
        cwd=ROOT, env=environment, check=True, stdout=sys.stderr)
    name = "agent-browser.exe" if target.endswith("-windows-msvc") else "agent-browser"
    program = folder / "target" / target / "release" / name
    if str(Path.home()).encode() in program.read_bytes():
        raise SystemExit(f"{program} names the home folder of the build")
    return program


def notices(folder: Path, target: str, output: Path) -> None:
    shutil.rmtree(output, ignore_errors=True)
    output.mkdir(parents=True)
    shutil.copyfile(folder / "LICENSE", output / "LICENSE")
    shutil.copyfile(ROOT / "AgentBrowser.lock", output / "AgentBrowser.lock")
    subprocess.run(
        [sys.executable, str(ROOT / "clients/qt/scripts/release/generate-rust-license-inventory.py"),
         "--manifest", str(folder / "cli/Cargo.toml"), "--target", target,
         "--root", "agent-browser",
         "--overrides", str(ROOT / "clients/qt/packaging/licenses/agent-browser-license-overrides.json"),
         "--output-dir", str(output / "rust")],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, help="the Rust target, as aarch64-apple-darwin")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--notices", type=Path)
    arguments = parser.parse_args()
    pinned = pin()
    folder = source(pinned["commit"], pinned["version"])
    program = build(folder, arguments.target)
    arguments.output_dir.mkdir(parents=True, exist_ok=True)
    destination = arguments.output_dir / program.name
    if not destination.exists() or not filecmp.cmp(program, destination, shallow=False):
        shutil.copy2(program, destination)
    if arguments.notices:
        notices(folder, arguments.target, arguments.notices)
    print(destination)


if __name__ == "__main__":
    main()
