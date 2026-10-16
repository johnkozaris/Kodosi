#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Iterator


def fail(message: str) -> None:
    raise SystemExit(f"[!] digest: {message}")


def bytes_sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as error:
        fail(f"cannot read {path}: {error.strerror}")
    return digest.hexdigest()


def artifact_paths(root: Path, prefix: str = "") -> Iterator[str]:
    try:
        entries = list(os.scandir(root / prefix if prefix else root))
    except OSError as error:
        fail(f"cannot read {root / prefix}: {error.strerror}")
    for entry in entries:
        relative = f"{prefix}{entry.name}"
        if entry.is_symlink() or entry.is_file(follow_symlinks=False):
            yield relative
        elif entry.is_dir(follow_symlinks=False):
            yield from artifact_paths(root, f"{relative}/")


def artifact_sha256(root: Path) -> str:
    if not root.is_dir():
        fail(f"artifact folder not found: {root}")
    lines = []
    for relative in sorted(artifact_paths(root), key=os.fsencode):
        path = root / relative
        name = os.fsencode(f"./{relative}")
        if path.is_symlink():
            lines.append(b"symlink  %s -> %s\n" % (name, os.fsencode(os.readlink(path))))
        else:
            lines.append(b"%s  %s\n" % (file_sha256(path).encode("ascii"), name))
    return bytes_sha256(b"".join(lines))


def tree_sha256(root: Path) -> str:
    if not root.is_dir():
        fail(f"tree not found: {root}")
    digest = hashlib.sha256()
    files = (item for item in root.rglob("*") if item.is_file())
    for path in sorted(files, key=lambda item: item.relative_to(root).parts):
        relative = path.relative_to(root).as_posix().encode()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        data = path.read_bytes()
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def git(source: Path, *arguments: str, environment: dict[str, str] | None = None) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(source), *arguments],
        env=environment,
        stdout=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"git {' '.join(arguments)} failed in {source}")
    return result.stdout


def patch_application_sha256(source: Path, commit: str) -> str:
    if not (source / ".git").is_dir():
        fail(f"patched source is not a Git worktree: {source}")
    if git(source, "rev-parse", "HEAD").decode().strip() != commit:
        fail(f"patched source {source} does not use the Ghostty commit {commit}")
    with tempfile.TemporaryDirectory(prefix="kodosi-ghostty-index.") as temporary:
        environment = dict(os.environ, GIT_INDEX_FILE=str(Path(temporary) / "index"))
        git(source, "read-tree", "HEAD", environment=environment)
        git(source, "-c", f"core.excludesFile={os.devnull}", "add", "--all", environment=environment)
        unchanged = subprocess.run(
            ["git", "-C", str(source), "diff-index", "--cached", "--quiet", "HEAD"],
            env=environment,
            check=False,
        ).returncode
        if unchanged == 0:
            fail(f"patched source {source} has no patch application")
        if unchanged != 1:
            fail(f"git diff-index failed in {source}")
        patch = git(
            source, "diff-index", "--cached", "--patch", "--binary", "--full-index", "--histogram",
            "--indent-heuristic", "HEAD", environment=environment,
        )
        return bytes_sha256(patch)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Write the SHA-256 digest that the Ghostty build records and the pin verifier checks.",
    )
    kinds = parser.add_subparsers(dest="kind", required=True)
    kinds.add_parser("file", help="the digest of one file").add_argument("path", type=Path)
    kinds.add_parser(
        "artifact",
        help="the digest of the files and symbolic links in a folder",
    ).add_argument("path", type=Path)
    kinds.add_parser("tree", help="the digest of the files in a Zig package store").add_argument(
        "path", type=Path
    )
    patch_application = kinds.add_parser(
        "patch-application",
        help="the digest of the patches that a patched Ghostty source applies to its commit",
    )
    patch_application.add_argument("path", type=Path)
    patch_application.add_argument("commit")
    arguments = parser.parse_args()
    sys.stdout.reconfigure(newline="\n")
    if arguments.kind == "file":
        if not arguments.path.is_file():
            fail(f"file not found: {arguments.path}")
        print(file_sha256(arguments.path))
    elif arguments.kind == "artifact":
        print(artifact_sha256(arguments.path))
    elif arguments.kind == "tree":
        print(tree_sha256(arguments.path))
    else:
        print(patch_application_sha256(arguments.path, arguments.commit))


if __name__ == "__main__":
    main()
