#!/usr/bin/env python3
"""Fetches the pinned ConPTY package and copies one architecture's console host.

Kodosi ships conpty.dll and OpenConsole.exe beside its executables and loads
them by absolute path, so every Windows build gets the same console host.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
LOCK = ROOT / "ConPTY.lock"
FILES = {
    "x64": ("runtimes/win-x64/native/conpty.dll", "build/native/runtimes/x64/OpenConsole.exe"),
    "arm64": ("runtimes/win-arm64/native/conpty.dll", "build/native/runtimes/arm64/OpenConsole.exe"),
}


def read_lock() -> dict[str, str]:
    values = dict(
        line.split("=", 1) for line in LOCK.read_text(encoding="utf-8").splitlines() if line.strip()
    )
    for key in ("package", "version", "url", "sha256"):
        if not values.get(key):
            raise SystemExit(f"ConPTY.lock is missing {key}")
    return values


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def package(lock: dict[str, str]) -> Path:
    cache = ROOT / ".cache" / f"{lock['package']}.{lock['version']}.nupkg"
    if cache.is_file() and sha256(cache) == lock["sha256"]:
        return cache
    cache.parent.mkdir(parents=True, exist_ok=True)
    partial = cache.with_suffix(".partial")
    with urllib.request.urlopen(lock["url"], timeout=120) as response, partial.open("wb") as file:
        shutil.copyfileobj(response, file)
    actual = sha256(partial)
    if actual != lock["sha256"]:
        partial.unlink()
        raise SystemExit(f"ConPTY package digest mismatch: expected {lock['sha256']}, found {actual}")
    partial.replace(cache)
    return cache


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", choices=sorted(FILES), required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    lock = read_lock()
    arguments.output.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(package(lock)) as archive:
        files = {Path(member).name: archive.read(member) for member in FILES[arguments.arch]}
    files["ConPTY-LICENSE.txt"] = (ROOT / "LICENSE").read_bytes()
    for name, content in files.items():
        target = arguments.output / name
        # Windows cannot replace a DLL that a running host has loaded, so an
        # unchanged file stays untouched.
        if not target.is_file() or target.read_bytes() != content:
            target.write_bytes(content)
    print(f"ConPTY {lock['version']} ({arguments.arch}) -> {arguments.output}")


if __name__ == "__main__":
    main()
