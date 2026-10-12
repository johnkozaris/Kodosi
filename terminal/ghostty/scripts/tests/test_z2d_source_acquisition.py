#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERIFIER = Path("Script/verify-third-party-notices.py")
INVENTORY = Path("ThirdPartyNotices/inventory.json")
ARCHIVE = Path(
    "ThirdPartyNotices/corresponding-source/"
    "z2d-7dbae85c81784dba9988320bf9543ed9a81350c8.tar.gz"
)


def verifier_result(root: Path) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    environment["KODOSI_WORKING_TREE_VALIDATION"] = "1"
    return subprocess.run(
        ["python3", str(VERIFIER)],
        cwd=root,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )


def require_rejection(root: Path, expected: str) -> None:
    result = verifier_result(root)
    if result.returncode == 0 or expected not in result.stderr:
        raise SystemExit(
            f"expected rejection containing {expected!r}; "
            f"status={result.returncode}\nstdout={result.stdout}\nstderr={result.stderr}"
        )


def copy_tree(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        symlinks=True,
        ignore=shutil.ignore_patterns(".build", ".git", ".tools", "build", "References"),
    )


def mutate_inventory(root: Path, mutation) -> None:
    path = root / INVENTORY
    inventory = json.loads(path.read_text())
    mutation(inventory)
    path.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n")


def main() -> None:
    baseline = verifier_result(ROOT)
    if baseline.returncode != 0:
        raise SystemExit(baseline.stderr)

    with tempfile.TemporaryDirectory(prefix="kodosi-z2d-verifier-") as temporary:
        base = Path(temporary)

        wrong_url = base / "wrong-url"
        copy_tree(wrong_url)
        mutate_inventory(
            wrong_url,
            lambda data: data["sourceAcquisitions"]["z2d"].__setitem__(
                "primaryUrl", "https://example.invalid/z2d.tar.gz"
            ),
        )
        require_rejection(wrong_url, "z2d acquisition metadata differs")

        wrong_commit = base / "wrong-commit"
        copy_tree(wrong_commit)
        mutate_inventory(
            wrong_commit,
            lambda data: data["sourceAcquisitions"]["z2d"].__setitem__(
                "commit", "0" * 40
            ),
        )
        require_rejection(wrong_commit, "z2d acquisition metadata differs")

        missing = base / "missing"
        copy_tree(missing)
        (missing / ARCHIVE).unlink()
        require_rejection(missing, "missing or unsafe tracked file")

        changed = base / "changed"
        copy_tree(changed)
        archive = changed / ARCHIVE
        archive.write_bytes(archive.read_bytes() + b"changed")
        require_rejection(changed, "compliance artifact changed")

        wrong_digest = base / "wrong-digest"
        copy_tree(wrong_digest)
        mutate_inventory(
            wrong_digest,
            lambda data: data["complianceArtifacts"][str(ARCHIVE)].__setitem__(
                "sha256", "0" * 64
            ),
        )
        require_rejection(wrong_digest, "compliance artifact changed")

    print("[*] z2d source-acquisition verifier mutation tests passed")


if __name__ == "__main__":
    main()
