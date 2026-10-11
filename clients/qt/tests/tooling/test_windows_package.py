from __future__ import annotations

import importlib.util
import struct
import unittest
import zipfile
from io import BytesIO
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "verify_windows_package", ROOT / "scripts/release/verify-windows-package.py"
)
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)
MACHINES = {"x64": 0x8664, "arm64": 0xAA64}


def portable_executable(machine: int) -> bytes:
    image = bytearray(0x80)
    image[:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, 0x40)
    image[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<H", image, 0x44, machine)
    return bytes(image)


def manifest(arch: str) -> bytes:
    text = (ROOT / "packaging/windows/AppxManifest.xml.in").read_text(encoding="utf-8")
    for name, value in {
        "KODOSI_MSIX_PUBLISHER_XML": "CN=Kodosi Test, O=&quot;Kodosi, Inc.&quot;",
        "KODOSI_MSIX_VERSION": "0.1.0.0",
        "KODOSI_WINDOWS_ARCH": arch,
    }.items():
        text = text.replace(f"@{name}@", value)
    return text.encode("utf-8")


def package(arch: str) -> dict[str, bytes]:
    files = {
        name: portable_executable(MACHINES[arch]) if name.endswith((".exe", ".dll")) else b"file"
        for name in verifier.REQUIRED_FILES
    }
    for logo in ("StoreLogo", "Square150x150Logo", "Square44x44Logo"):
        files[f"Assets/{logo}.png"] = b"png"
    files["AppxManifest.xml"] = manifest(arch)
    return files


def verify(files: dict[str, bytes], arch: str) -> None:
    buffer = BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    with zipfile.ZipFile(buffer) as archive:
        entries = verifier.package_files(archive)

        def read(name: str) -> bytes:
            return archive.read(entries[name])

        logos = verifier.verify_manifest(read("AppxManifest.xml"), manifest(arch), arch)
        verifier.verify_payload(entries, read, logos, arch)


class WindowsPackageTests(unittest.TestCase):
    def test_accepts_complete_packages(self) -> None:
        for arch in MACHINES:
            verify(package(arch), arch)

    def test_rejects_another_architecture(self) -> None:
        files = package("x64")
        files["plugins/platforms/qwindows.dll"] = portable_executable(MACHINES["arm64"])
        with self.assertRaises(SystemExit):
            verify(files, "x64")
        with self.assertRaises(SystemExit):
            verify(package("x64"), "arm64")

    def test_rejects_missing_runtime_and_build_files(self) -> None:
        files = package("x64")
        del files["vcruntime140.dll"]
        with self.assertRaises(SystemExit):
            verify(files, "x64")
        files = package("x64")
        files["kodosi-qt.pdb"] = b""
        with self.assertRaises(SystemExit):
            verify(files, "x64")

    def test_rejects_capabilities_beyond_full_trust(self) -> None:
        widened = manifest("x64").replace(
            b'<rescap:Capability Name="runFullTrust" />',
            b'<rescap:Capability Name="runFullTrust" />\n    <Capability Name="internetClient" />',
        )
        with self.assertRaises(SystemExit):
            verifier.verify_manifest(widened, widened, "x64")
        with self.assertRaises(SystemExit):
            verifier.verify_manifest(widened, manifest("x64"), "x64")

    def test_reads_percent_encoded_part_names(self) -> None:
        buffer = BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("doc/rust/a%20b%2Bc.txt", b"")
        with zipfile.ZipFile(buffer) as archive:
            self.assertEqual(list(verifier.package_files(archive)), ["doc/rust/a b+c.txt"])


if __name__ == "__main__":
    unittest.main()
