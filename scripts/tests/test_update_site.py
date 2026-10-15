from __future__ import annotations

import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "generate-update-site.py"
VERSION = "1.2.3"


def ar_member(name: str, data: bytes) -> bytes:
    header = f"{name:<16}{0:<12}{0:<6}{0:<6}{'100644':<8}{len(data):<10}`\n".encode("ascii")
    return header + data + (b"\n" if len(data) % 2 else b"")


def debian_package(version: str) -> bytes:
    control = f"Package: kodosi\nVersion: {version}\nArchitecture: amd64\nMaintainer: Kodosi\nDescription: Kodosi\n"
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        info = tarfile.TarInfo("./control")
        info.size = len(control)
        archive.addfile(info, io.BytesIO(control.encode("utf-8")))
    return b"!<arch>\n" + ar_member("debian-binary", b"2.0\n") + ar_member("control.tar.gz", buffer.getvalue())


def windows_bundle(version: str) -> bytes:
    appinstaller = (
        '<AppInstaller xmlns="http://schemas.microsoft.com/appx/appinstaller/2021" Version="1.0.0.0" '
        'Uri="https://updates.kodosi.com/windows/Kodosi.appinstaller"><MainBundle Name="com.kodosi.Kodosi" '
        f'Publisher="CN=Kodosi" Version="{version}.0" '
        f'Uri="https://github.com/johnkozaris/Kodosi/releases/download/v{version}/Kodosi-{version}.msixbundle"/>'
        "</AppInstaller>"
    )
    bundle = io.BytesIO()
    with zipfile.ZipFile(bundle, "w") as outer:
        for arch in ("x64", "arm64"):
            package = io.BytesIO()
            with zipfile.ZipFile(package, "w") as inner:
                inner.writestr("Kodosi.appinstaller", appinstaller)
            outer.writestr(f"kodosi-{VERSION}-windows-{arch}.msix", package.getvalue())
    return bundle.getvalue()


def generate(root: Path, package_version: str, windows_version: str | None = None) -> subprocess.CompletedProcess[bytes]:
    release = root / "release"
    release.mkdir()
    if windows_version:
        (release / f"Kodosi-{VERSION}.msixbundle").write_bytes(windows_bundle(windows_version))
    (release / f"Kodosi-{VERSION}.dmg").write_bytes(b"disk image")
    (release / f"kodosi_{VERSION}_amd64.deb").write_bytes(debian_package(package_version))
    (release / f"kodosi-{VERSION}-linux-x86_64.tar.gz").write_bytes(b"archive")
    (release / f"kodosi-bin-{VERSION}-1-x86_64.pkg.tar.zst").write_bytes(b"arch package")
    (root / "appcast.xml").write_text("<rss/>", encoding="utf-8")
    return subprocess.run(
        [sys.executable, str(SCRIPT), str(release), str(root / "site"), "--version", VERSION,
         "--appcast", str(root / "appcast.xml")],
        capture_output=True,
    )


class UpdateSiteTest(unittest.TestCase):
    def test_the_site_lists_the_release_and_sends_downloads_to_github(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.assertEqual(generate(root, VERSION).returncode, 0)
            site = root / "site"
            packages = (site / "apt/dists/stable/main/binary-amd64/Packages").read_bytes()
            self.assertIn(f"Filename: pool/{VERSION}/kodosi_{VERSION}_amd64.deb".encode(), packages)
            release = (site / "apt/dists/stable/Release").read_text(encoding="utf-8")
            self.assertIn(f" {hashlib.sha256(packages).hexdigest()} {len(packages)} main/binary-amd64/Packages\n", release)
            redirects = (site / "_redirects").read_text(encoding="utf-8")
            download = f"https://github.com/johnkozaris/Kodosi/releases/download/v{VERSION}"
            self.assertIn(f"/download/macos {download}/Kodosi-{VERSION}.dmg 302\n", redirects)
            self.assertIn("/apt/pool/:version/:file https://github.com/johnkozaris/Kodosi/releases/download/v:version/:file 302\n", redirects)
            listing = json.loads((site / "release.json").read_text(encoding="utf-8"))
            self.assertEqual(listing["files"]["linux"]["url"], f"{download}/kodosi_{VERSION}_amd64.deb")
            sums = (root / "release/SHA256SUMS").read_text(encoding="utf-8")
            self.assertIn(f"{hashlib.sha256(b'disk image').hexdigest()}  Kodosi-{VERSION}.dmg\n", sums)
            self.assertEqual((site / "macos/appcast.xml").read_text(encoding="utf-8"), "<rss/>")

    def test_a_package_of_another_version_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            assert_rejected(self, generate(Path(temporary), "1.2.2"))

    def test_a_windows_bundle_gives_the_update_file_that_its_packages_embed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.assertEqual(generate(root, VERSION, VERSION).returncode, 0)
            site = root / "site"
            appinstaller = (site / "windows/Kodosi.appinstaller").read_text(encoding="utf-8")
            self.assertIn(f'Version="{VERSION}.0"', appinstaller)
            self.assertIn("Content-Type: application/appinstaller", (site / "_headers").read_text(encoding="utf-8"))
            download = f"https://github.com/johnkozaris/Kodosi/releases/download/v{VERSION}"
            redirects = (site / "_redirects").read_text(encoding="utf-8")
            self.assertIn(f"/download/windows {download}/Kodosi-{VERSION}.msixbundle 302\n", redirects)
            listing = json.loads((site / "release.json").read_text(encoding="utf-8"))
            self.assertEqual(listing["files"]["windows"]["name"], f"Kodosi-{VERSION}.msixbundle")

    def test_a_windows_bundle_of_another_version_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            assert_rejected(self, generate(Path(temporary), VERSION, "1.2.2"))


def assert_rejected(test: unittest.TestCase, result: subprocess.CompletedProcess[bytes]) -> None:
    test.assertNotEqual(result.returncode, 0, "the generator accepted a package of another version")


if __name__ == "__main__":
    unittest.main()
