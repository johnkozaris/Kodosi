"""The values that a release reads from the Kodosi checkout: the source
identity of the build and the product version."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


def git(root: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError(detail or f"git {arguments[0]} failed")
    return result.stdout.decode("utf-8")


def repository_root(path: Path) -> Path:
    root = git(path.resolve(strict=True), "rev-parse", "--show-toplevel").strip()
    return Path(root).resolve(strict=True)


def repository_name(root: Path) -> str:
    remote = git(root, "remote", "get-url", "origin").strip()
    match = re.fullmatch(
        r"(?:(?:https?://|ssh://git@)github\.com/|git@github\.com:)([^/]+/[^/]+?)(?:\.git)?",
        remote,
    )
    if match is None:
        raise ValueError("the remote origin must be a GitHub repository")
    return match.group(1)


def checkout_dirty(root: Path) -> bool:
    # git status does not show a change to a file with the assume-unchanged or
    # the skip-worktree flag. git ls-files -v gives these files a lowercase tag
    # or the tag S.
    tags = (entry[:1] for entry in git(root, "ls-files", "-v", "-z").split("\0") if entry)
    if any(tag == "S" or tag.islower() for tag in tags):
        return True
    return git(root, "status", "--porcelain", "-z", "--untracked-files=all") != ""


def commit_epoch(path: Path) -> int:
    value = git(path.resolve(strict=True), "show", "-s", "--format=%ct", "HEAD").strip()
    if not re.fullmatch(r"[0-9]+", value) or int(value) <= 0:
        raise ValueError("the commit time of HEAD is malformed")
    return int(value)


def source_identity(path: Path) -> dict[str, object]:
    root = repository_root(path)
    commit = git(root, "rev-parse", "--verify", "HEAD").strip()
    if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", commit):
        raise ValueError("HEAD is not a full lowercase Git commit")
    return {
        "schemaVersion": 4,
        "sourceDateEpoch": commit_epoch(root),
        "repository": repository_name(root),
        "commit": commit,
        "dirty": checkout_dirty(root),
    }


def product_version(source_root: Path) -> str:
    cmake = (source_root / "CMakeLists.txt").read_text(encoding="utf-8")
    match = re.search(
        r"\bproject\s*\(\s*KodosiQT\b[\s\S]*?\bVERSION\s+"
        r"([0-9]+\.[0-9]+\.[0-9]+)\b",
        cmake,
    )
    if not match:
        raise ValueError("KodosiQT product version was not found")
    return match.group(1)
