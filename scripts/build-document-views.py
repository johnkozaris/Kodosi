#!/usr/bin/env python3
"""Builds the view bundle of live documents, when its sources changed, and copies it."""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

VIEWS = Path(__file__).resolve().parent.parent / "documents" / "views"
BUILT = VIEWS / "dist"
BUILT_PAGE = BUILT / "index.html"
NOT_SOURCES = {"node_modules", "dist"}


def newest_source() -> float:
    newest = 0.0
    for folder, folders, files in os.walk(VIEWS):
        folders[:] = [name for name in folders if name not in NOT_SOURCES]
        for name in files:
            newest = max(newest, (Path(folder) / name).stat().st_mtime)
    return newest


def build() -> None:
    bun = shutil.which("bun")
    if bun is None:
        sys.exit("Install Bun to build the document views.")
    subprocess.run([bun, "install", "--frozen-lockfile"], cwd=VIEWS, check=True)
    subprocess.run([bun, "run", "build"], cwd=VIEWS, check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    if not BUILT_PAGE.exists() or BUILT_PAGE.stat().st_mtime < newest_source():
        build()
    shutil.rmtree(arguments.output_dir, ignore_errors=True)
    shutil.copytree(BUILT, arguments.output_dir)


if __name__ == "__main__":
    main()
