#!/usr/bin/env python3

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.dont_write_bytecode = True
GHOSTTY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(GHOSTTY / "scripts"))

import digest  # noqa: E402

FILES = {
    "a/b/one": b"x\n",
    "a-c/two": b"y",
    "B/empty": b"",
    "top.txt": b"top\n",
    "dir with space/file name": b"z\n",
}
TREE = "5b817b3b1f61341b437a29a970c0b65e447343783c60197da164970ca0e7fe55"
PATCH_APPLICATION = "9b46920555ca5b3ed1f9ce8c3d8beb22c1c1bfe9109ac4d1297819b7c59065a2"


class DigestTests(unittest.TestCase):
    def setUp(self) -> None:
        build = GHOSTTY / "build"
        build.mkdir(exist_ok=True)
        self.temporary = TemporaryDirectory(prefix="digest-", dir=build)
        self.root = Path(self.temporary.name)
        for relative, payload in FILES.items():
            path = self.root / "tree" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_tree_digest_keeps_its_value(self) -> None:
        self.assertEqual(digest.tree_sha256(self.root / "tree"), TREE)

    @mock.patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
    def test_patch_application_digest_keeps_its_value(self) -> None:
        source = self.root / "source"
        source.mkdir()

        def git(*arguments: str) -> str:
            return subprocess.run(
                ["git", "-C", str(source), "-c", "user.name=Kodosi", "-c", "user.email=test@kodosi.invalid", *arguments],
                stdout=subprocess.PIPE,
                universal_newlines=True,
                check=True,
            ).stdout.strip()

        git("init", "-q")
        (source / ".gitignore").write_bytes(b"zig-pkg/\n")
        (source / "build.zig").write_bytes(b"upstream\n")
        git("add", "--all")
        git("commit", "-q", "-m", "upstream")
        commit = git("rev-parse", "HEAD")
        (source / "build.zig").write_bytes(b"patched\n")
        (source / "added.zig").write_bytes(b"new\n")
        (source / "zig-pkg").mkdir()
        (source / "zig-pkg" / "x").write_bytes(b"ignored\n")
        self.assertEqual(digest.patch_application_sha256(source, commit), PATCH_APPLICATION)


if __name__ == "__main__":
    unittest.main()
