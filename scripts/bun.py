#!/usr/bin/env python3
"""Prints the path of the Bun of .bun-version in .tools/bun, which it installs there first if needed."""

# Xcode runs this with its own Python 3.9.
import hashlib
import platform
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = (ROOT / ".bun-version").read_text("utf-8").strip()
TOOLS = ROOT / ".tools" / "bun"
RELEASE = f"https://github.com/oven-sh/bun/releases/download/bun-v{VERSION}"
SYSTEMS = {"darwin": "darwin", "linux": "linux", "win32": "windows"}


def bun() -> Path:
    program = TOOLS / VERSION / ("bun.exe" if sys.platform == "win32" else "bun")
    if not program.is_file():
        install(program)
    return program


def install(program: Path) -> None:
    if sys.platform not in SYSTEMS:
        sys.exit(f"Bun has no build for {sys.platform}.")
    machine = "aarch64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
    name = f"bun-{SYSTEMS[sys.platform]}-{machine}"
    with tempfile.TemporaryDirectory() as folder:
        archive = Path(folder) / f"{name}.zip"
        sums = Path(folder) / "SHASUMS256.txt"
        download(f"{RELEASE}/{name}.zip", archive)
        download(f"{RELEASE}/SHASUMS256.txt", sums)
        listed = [line.split()[0] for line in sums.read_text("utf-8").splitlines() if line.endswith(f" {name}.zip")]
        if listed != [hashlib.sha256(archive.read_bytes()).hexdigest()]:
            sys.exit(f"The download of Bun {VERSION} differs from its release.")
        with zipfile.ZipFile(archive) as files:
            data = files.read(f"{name}/{program.name}")
    for other in TOOLS.glob("*"):
        if other.name != VERSION:
            shutil.rmtree(other, ignore_errors=True)
    program.parent.mkdir(parents=True, exist_ok=True)
    part = program.with_name(f"{program.name}.part")
    part.write_bytes(data)
    part.chmod(0o755)
    part.replace(program)


def download(address: str, target: Path) -> None:
    subprocess.run(["curl", "--fail", "--silent", "--show-error", "--location", "--output", str(target), address], check=True)


if __name__ == "__main__":
    print(bun())
