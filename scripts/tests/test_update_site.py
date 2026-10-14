import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
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


def generate(root: Path, package_version: str) -> subprocess.CompletedProcess[bytes]:
    release = root / "release"
    release.mkdir()
    (release / f"Kodosi-{VERSION}.dmg").write_bytes(b"disk image")
    (release / f"kodosi_{VERSION}_amd64.deb").write_bytes(debian_package(package_version))
    (release / f"kodosi-{VERSION}-linux-x86_64.tar.gz").write_bytes(b"archive")
    (release / f"kodosi-bin-{VERSION}-1-x86_64.pkg.tar.zst").write_bytes(b"arch package")
    (root / "appcast.xml").write_text("<rss/>", encoding="utf-8")
    (root / "key.pgp").write_bytes(b"public key")
    return subprocess.run(
        [sys.executable, str(SCRIPT), str(release), str(root / "site"), "--version", VERSION,
         "--appcast", str(root / "appcast.xml"), "--apt-key", str(root / "key.pgp")],
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


def assert_rejected(test: unittest.TestCase, result: subprocess.CompletedProcess[bytes]) -> None:
    test.assertNotEqual(result.returncode, 0, "the generator accepted a package of another version")


if __name__ == "__main__":
    unittest.main()
