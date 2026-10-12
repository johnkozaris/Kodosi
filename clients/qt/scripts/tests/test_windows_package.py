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
WINDOWS = {"kernel32.dll", "user32.dll", "msvcp_win.dll", "vcruntime140_1.dll"}
APP_IMPORTS = ("Qt6Core.dll", "Qt6Widgets.dll", "VCRUNTIME140.dll", "KERNEL32.dll", "api-ms-win-crt-runtime-l1-1-0.dll")


def portable_executable(machine: int, imports: tuple[str, ...] = (), delayed: tuple[str, ...] = ()) -> bytes:
    """A PE32+ image whose one section holds its import and delay-import tables."""
    optional, optional_size, section_rva, section_raw = 0x58, 240, 0x1000, 0x200
    tables = bytearray((len(imports) + 1) * 20 + (len(delayed) + 1) * 32)
    for index, name in enumerate((*imports, *delayed)):
        rva = section_rva + len(tables)
        tables += name.encode("ascii") + b"\0"
        if index < len(imports):
            struct.pack_into("<I", tables, index * 20 + 12, rva)
        else:
            struct.pack_into("<I", tables, (len(imports) + 1) * 20 + (index - len(imports)) * 32 + 4, rva)
    image = bytearray(section_raw)
    image[:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, 0x40)
    image[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<HH", image, 0x44, machine, 1)
    struct.pack_into("<H", image, 0x54, optional_size)
    struct.pack_into("<H", image, optional, 0x20B)
    struct.pack_into("<I", image, optional + 108, 16)
    if imports:
        struct.pack_into("<I", image, optional + 112 + 8, section_rva)
    if delayed:
        struct.pack_into("<I", image, optional + 112 + 13 * 8, section_rva + (len(imports) + 1) * 20)
    struct.pack_into("<IIII", image, optional + optional_size + 8, len(tables), section_rva, len(tables), section_raw)
    return bytes(image + tables)


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
    files["kodosi-qt.exe"] = portable_executable(MACHINES[arch], APP_IMPORTS, ("USER32.dll",))
    for library in ("Qt6Core.dll", "Qt6Widgets.dll", "vcruntime140.dll"):
        files[library] = portable_executable(MACHINES[arch], ("KERNEL32.dll",))
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
        verifier.verify_imports(entries, read, WINDOWS.__contains__)


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

    def test_reads_imports_and_delayed_imports(self) -> None:
        self.assertEqual(
            verifier.pe_imports(portable_executable(MACHINES["x64"], ("A.dll", "b.DLL"), ("C.dll",))),
            {"a.dll", "b.dll", "c.dll"},
        )
        self.assertEqual(verifier.pe_imports(portable_executable(MACHINES["arm64"])), set())
        with self.assertRaises(SystemExit):
            verifier.pe_imports(portable_executable(MACHINES["x64"], ("A.dll",))[:-4])

    def test_every_import_is_packaged_or_part_of_windows(self) -> None:
        for library in ("Qt6Widgets.dll", "Qt6Core.dll"):
            files = package("x64")
            del files[library]
            with self.subTest(library=library), self.assertRaises(SystemExit):
                verify(files, "x64")
        files = package("x64")
        files["plugins/platforms/qwindows.dll"] = portable_executable(MACHINES["x64"], ("Qt6Gui.dll",))
        with self.assertRaises(SystemExit):
            verify(files, "x64")
        files["Qt6Gui.dll"] = portable_executable(MACHINES["x64"])
        verify(files, "x64")
        files["kodosi.exe"] = portable_executable(MACHINES["x64"], (), ("missing.dll",))
        with self.assertRaises(SystemExit):
            verify(files, "x64")

    def test_the_c_runtime_comes_from_the_package_even_when_windows_has_one(self) -> None:
        files = package("x64")
        files["kodosi.exe"] = portable_executable(MACHINES["x64"], ("vcruntime140_1.dll",))
        with self.assertRaises(SystemExit):
            verify(files, "x64")
        files["vcruntime140_1.dll"] = portable_executable(MACHINES["x64"])
        verify(files, "x64")
        files["OpenConsole.exe"] = portable_executable(MACHINES["x64"], ("msvcp_win.dll",))
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
