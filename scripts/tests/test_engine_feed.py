from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "generate-engine-feed.py"
VERSION = "156.0.8078.80-0.1.0"


def load():
    spec = importlib.util.spec_from_file_location("generate_engine_feed", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def package(folder: Path, platform: str, program: str, version: str = VERSION, data: bytes = b"engine") -> Path:
    archive = folder / f"kodosi-browser-{version}-{platform}.tar.gz"
    archive.write_bytes(data)
    entry = {"version": version, "platform": platform, "file": archive.name, "program": program,
             "sha256": hashlib.sha256(data).hexdigest()}
    (folder / f"{archive.name}.json").write_text(json.dumps(entry), encoding="utf-8")
    return archive


class EngineFeedTest(unittest.TestCase):
    def setUp(self) -> None:
        self.script = load()

    def test_the_feed_names_each_package_by_its_download_in_the_browser_release(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            package(folder, "mac-arm64", "Kodosi Browser.app")
            package(folder, "linux-x64", "kodosi-browser/kodosi-browser", data=b"linux")
            package(folder, "windows-x64", "kodosi-browser/kodosi-browser.exe", data=b"windows")
            feed = self.script.feed(folder)
            self.assertEqual(feed["version"], VERSION)
            self.assertEqual(sorted(feed["engines"]), ["linux-x64", "mac-arm64", "windows-x64"])
            linux = feed["engines"]["linux-x64"]
            self.assertEqual(
                linux["url"],
                f"https://github.com/johnkozaris/Kodosi/releases/download/browser-{VERSION}/kodosi-browser-{VERSION}-linux-x64.tar.gz",
            )
            self.assertEqual(linux["sha256"], hashlib.sha256(b"linux").hexdigest())
            self.assertEqual(linux["program"], "kodosi-browser/kodosi-browser")

    def test_packages_of_two_versions_are_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            package(folder, "mac-arm64", "Kodosi Browser.app")
            package(folder, "linux-x64", "kodosi-browser/kodosi-browser", version="156.0.1-0.1.0")
            with self.assertRaises(SystemExit):
                self.script.feed(folder)

    def test_a_package_that_does_not_match_its_entry_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            package(folder, "linux-x64", "kodosi-browser/kodosi-browser").write_bytes(b"changed")
            with self.assertRaises(SystemExit):
                self.script.feed(folder)

    def test_a_folder_without_packages_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, self.assertRaises(SystemExit):
            self.script.feed(Path(temporary))


if __name__ == "__main__":
    unittest.main()
