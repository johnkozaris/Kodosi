#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "Script" / "verify-platform-pins.py"
MAC_REF = "a" * 40
LINUX_REF = "b" * 40


class PlatformPinTests(unittest.TestCase):
    def setUp(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        self.temporary = TemporaryDirectory(
            prefix="platform-pins-",
            dir=build,
        )
        self.fixture = Path(self.temporary.name)
        self.write("MacOSGhostty.ref", MAC_REF + "\n")
        self.write("LinuxGhostty.ref", LINUX_REF + "\n")
        self.write(
            "Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a",
            "mac archive\n",
        )
        self.write(
            "Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a",
            "mac archive\n",
        )
        self.write(
            "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a",
            "linux archive\n",
        )
        mac_digest = self.digest(
            "Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
        )
        linux_digest = self.digest(
            "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
        )
        self.write(
            "Vendor/libghostty.version",
            f"ghostty_commit={MAC_REF}\n"
            f"combined_archive_sha256={mac_digest}\n",
        )
        self.write(
            "Vendor/GhosttyVt/linux-x86_64.version",
            f"ghostty_commit={LINUX_REF}\n"
            f"vt_archive_sha256={linux_digest}\n",
        )
        self.write_json(
            "ThirdPartyNotices/inventory.json",
            {"ghosttyCommit": MAC_REF},
        )
        self.write_json(
            "ThirdPartyNotices/linux-vt-inventory.json",
            {"ghosttyCommit": LINUX_REF},
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, relative: str, content: str) -> None:
        path = self.fixture / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def write_json(self, relative: str, value: dict) -> None:
        self.write(
            relative,
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
        )

    def digest(self, relative: str) -> str:
        return hashlib.sha256((self.fixture / relative).read_bytes()).hexdigest()

    def verify(self, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            ["python3", str(VERIFIER), "--root", str(self.fixture)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if expect_success and result.returncode != 0:
            self.fail(result.stderr or result.stdout)
        if not expect_success and result.returncode == 0:
            self.fail("platform pin verifier unexpectedly succeeded")
        return result

    def test_exact_platform_relationship_is_deterministic(self) -> None:
        self.assertEqual(self.verify().stdout, self.verify().stdout)

    def test_swapping_either_platform_ref_is_rejected(self) -> None:
        for relative, replacement in (
            ("MacOSGhostty.ref", LINUX_REF),
            ("LinuxGhostty.ref", MAC_REF),
        ):
            with self.subTest(relative=relative):
                original = (self.fixture / relative).read_bytes()
                self.write(relative, replacement + "\n")
                self.verify(expect_success=False)
                (self.fixture / relative).write_bytes(original)

    def test_wrong_platform_artifact_digests_are_rejected(self) -> None:
        for relative in (
            "Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a",
            "Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a",
            "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a",
        ):
            with self.subTest(relative=relative):
                path = self.fixture / relative
                original = path.read_bytes()
                path.write_bytes(original + b"substitution\n")
                self.verify(expect_success=False)
                path.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
