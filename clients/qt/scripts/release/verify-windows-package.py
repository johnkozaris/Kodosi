#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import re
import struct
import xml.etree.ElementTree as ElementTree
import zipfile
from collections.abc import Callable
from pathlib import Path, PurePosixPath
from urllib.parse import unquote

from package_evidence import verify_rust_licenses, verify_source_identity
from release_source import source_identity


ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = ROOT.parents[1]
MACHINES = {
    "x86_64-pc-windows-msvc": ("x64", 0x8664),
    "aarch64-pc-windows-msvc": ("arm64", 0xAA64),
}
NAMESPACES = {
    "m": "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
    "uap": "http://schemas.microsoft.com/appx/manifest/uap/windows10",
    "uap3": "http://schemas.microsoft.com/appx/manifest/uap/windows10/3",
    "desktop": "http://schemas.microsoft.com/appx/manifest/desktop/windows10",
    "rescap": "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities",
}
# Files nothing imports. The import check covers every DLL a packaged binary links.
REQUIRED_FILES = (
    "kodosi-qt.exe",
    "kodosi.exe",
    "conpty.dll",
    "OpenConsole.exe",
    "qt.conf",
    "plugins/platforms/qwindows.dll",
    "resources.pri",
)
# Windows may hold an older copy of the C runtime, so the package carries its own.
C_RUNTIME = re.compile(r"(vcruntime|msvcp|concrt|vccorlib)\d")
QT_SBOM_MODULES = ("qtbase", "qtdeclarative", "qtsvg")
QT_SBOM_FORMATS = ("spdx", "spdx.json", "cdx.json")

Reader = Callable[[str], bytes]


def fail(message: str) -> None:
    raise SystemExit(f"Windows package verification error: {message}")


def pe_machine(data: bytes) -> int | None:
    if len(data) < 0x40 or data[:2] != b"MZ":
        return None
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[offset : offset + 4] != b"PE\0\0" or len(data) < offset + 6:
        return None
    return struct.unpack_from("<H", data, offset + 4)[0]


def pe_imports(data: bytes) -> set[str]:
    try:
        coff = struct.unpack_from("<I", data, 0x3C)[0] + 4
        section_count, optional_size = struct.unpack_from("<H", data, coff + 2)[0], struct.unpack_from("<H", data, coff + 16)[0]
        optional = coff + 20
        directories = optional + (112 if struct.unpack_from("<H", data, optional)[0] == 0x20B else 96)
        directory_count = 0
        if optional_size >= directories - optional:
            directory_count = min(struct.unpack_from("<I", data, directories - 4)[0], (optional + optional_size - directories) // 8)
        sections = [struct.unpack_from("<IIII", data, optional + optional_size + 40 * index + 8) for index in range(section_count)]

        def offset(rva: int) -> int:
            for virtual_size, address, raw_size, raw in sections:
                if address <= rva < address + max(virtual_size, raw_size):
                    return raw + rva - address
            raise ValueError(f"address {rva:#x} is outside every section")

        names = set()
        for index, size, name_field in ((1, 20, 12), (13, 32, 4)):
            if index >= directory_count or not (address := struct.unpack_from("<I", data, directories + 8 * index)[0]):
                continue
            position = offset(address)
            while any(descriptor := data[position : position + size]):
                if len(descriptor) < size:
                    raise ValueError("an import table is truncated")
                start = offset(struct.unpack_from("<I", descriptor, name_field)[0])
                names.add(data[start : data.index(b"\0", start)].decode("ascii").lower())
                position += size
        return names
    except (struct.error, ValueError, UnicodeDecodeError) as error:
        fail(f"cannot read the imports of a packaged binary: {error}")


def windows_library(name: str) -> bool:
    root = os.environ.get("SystemRoot")
    if root is None:
        fail("verify the package on Windows, which supplies its system libraries")
    return (Path(root) / "System32" / name).is_file()


def verify_imports(files: dict[str, zipfile.ZipInfo], read: Reader, system: Callable[[str], bool]) -> None:
    packaged = {name.lower() for name in files if "/" not in name}
    for name in files:
        if not name.lower().endswith((".exe", ".dll")):
            continue
        for library in pe_imports(read(name)):
            if library in packaged or library.startswith(("api-ms-", "ext-ms-")):
                continue
            if C_RUNTIME.match(library) or not system(library):
                fail(f"{name} imports {library}, which the package does not provide")


def package_files(package: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    files: dict[str, zipfile.ZipInfo] = {}
    for info in package.infolist():
        if info.is_dir():
            continue
        name = unquote(info.filename)
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\\" in name:
            fail(f"unsafe package path: {info.filename}")
        files[name] = info
    return files


def verify_manifest(manifest: bytes, configured: bytes, arch: str) -> list[str]:
    if manifest != configured:
        fail("AppxManifest.xml differs from the configured manifest")
    root = ElementTree.fromstring(manifest)
    identity = root.find("m:Identity", NAMESPACES)
    if identity is None or identity.get("ProcessorArchitecture") != arch:
        fail(f"the package identity is not {arch}")
    family = root.find("m:Dependencies/m:TargetDeviceFamily", NAMESPACES)
    if family is None or family.get("Name") != "Windows.Desktop" or family.get("MinVersion") != "10.0.22000.0":
        fail("the package must target Windows 11 desktops")
    applications = root.findall("m:Applications/m:Application", NAMESPACES)
    if len(applications) != 1 or applications[0].get("Executable") != "kodosi-qt.exe":
        fail("the package must have one application, kodosi-qt.exe")
    capabilities = root.find("m:Capabilities", NAMESPACES)
    if capabilities is None or [(element.tag, element.get("Name")) for element in capabilities] != [
        (f"{{{NAMESPACES['rescap']}}}Capability", "runFullTrust")
    ]:
        fail("runFullTrust must be the only capability")
    protocol = applications[0].find(".//uap3:Protocol", NAMESPACES)
    if protocol is None or protocol.get("Name") != "kodosi":
        fail("the kodosi protocol is missing")
    alias = applications[0].find(".//uap3:Extension[@Category='windows.appExecutionAlias']", NAMESPACES)
    alias_name = None if alias is None else alias.find("uap3:AppExecutionAlias/desktop:ExecutionAlias", NAMESPACES)
    if alias is None or alias.get("Executable") != "kodosi.exe" or alias_name is None or alias_name.get("Alias") != "kodosi.exe":
        fail("the kodosi.exe alias must run the CLI")
    visual = applications[0].find("uap:VisualElements", NAMESPACES)
    logo = root.find("m:Properties/m:Logo", NAMESPACES)
    if visual is None or logo is None or not logo.text:
        fail("the package logos are missing")
    logos = [logo.text, visual.get("Square150x150Logo"), visual.get("Square44x44Logo")]
    if not all(logos):
        fail("the package logos are missing")
    return [name.replace("\\", "/") for name in logos]


def verify_payload(files: dict[str, zipfile.ZipInfo], read: Reader, logos: list[str], arch: str) -> None:
    machine = dict(MACHINES.values())[arch]
    for name in (*REQUIRED_FILES, *logos):
        if name not in files:
            fail(f"missing {name}")
    for name in files:
        lower = name.lower()
        if lower.endswith((".pdb", ".ilk", ".exp", ".lib")) or PurePosixPath(lower).name.startswith(("vc_redist", "vcredist")):
            fail(f"unexpected build or installer file: {name}")
        if lower.endswith((".exe", ".dll")) and pe_machine(read(name)) != machine:
            fail(f"{name} is not an {arch} binary")


def same(files: dict[str, zipfile.ZipInfo], read: Reader, name: str, source: Path) -> None:
    if name not in files:
        fail(f"missing {name}")
    if read(name) != source.read_bytes():
        fail(f"{name} differs from {source}")


def verify_evidence(files: dict[str, zipfile.ZipInfo], read: Reader, build_dir: Path, qt_root: Path) -> None:
    same(files, read, "doc/provenance/dependencies.lock.json", ROOT / "dependencies.lock.json")
    ghostty = REPOSITORY / "terminal/ghostty"
    for name in ("LICENSE", "LICENSE-GHOSTTY", "WindowsGhostty.ref", "THIRD_PARTY_NOTICES.md"):
        same(files, read, f"doc/ghostty/{name}", ghostty / name)
    texts = [name for name in files if name.startswith("doc/ghostty/ThirdPartyNotices/licenses/")]
    if not texts:
        fail("the Ghostty license texts are missing")
    for name in texts:
        same(files, read, name, ghostty / "ThirdPartyNotices/licenses" / PurePosixPath(name).name)
    for name in ("LICENSE", "ConPTY.lock"):
        same(files, read, f"doc/conpty/{name}", REPOSITORY / "terminal/conpty" / name)
    for notice in sorted((ROOT / "packaging/licenses/provider-icons").iterdir()):
        same(files, read, f"doc/provider-icons/{notice.name}", notice)
    for name in ("conpty.dll", "OpenConsole.exe"):
        same(files, read, name, build_dir / "conpty" / name)

    qt_version = json.loads((ROOT / "dependencies.lock.json").read_text(encoding="utf-8"))["frameworks"]["qt"]["version"]
    qt_licenses = ROOT / "packaging/licenses/qt" / qt_version
    expected = {
        f"doc/qt/{path.relative_to(qt_licenses).as_posix()}": path
        for path in qt_licenses.rglob("*")
        if path.is_file()
    }
    for module in QT_SBOM_MODULES:
        for suffix in QT_SBOM_FORMATS:
            sbom = f"{module}-{qt_version}.{suffix}"
            expected[f"doc/qt/sbom/{sbom}"] = qt_root / "sbom" / sbom
    if {name for name in files if name.startswith("doc/qt/")} != set(expected):
        fail("the Qt notices and SBOMs are incomplete or have extra files")
    for name, source in expected.items():
        same(files, read, name, source)

    if "doc/provenance/source-identity.json" not in files:
        fail("missing doc/provenance/source-identity.json")
    try:
        verify_source_identity(read("doc/provenance/source-identity.json"), source_identity(ROOT))
        verify_rust_licenses(
            {name.removeprefix("doc/rust/"): read(name) for name in files if name.startswith("doc/rust/")},
            build_dir / "generated/rust-dependency-licenses",
        )
    except (OSError, ValueError) as error:
        fail(str(error))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=Path)
    parser.add_argument("--target", required=True, choices=sorted(MACHINES))
    parser.add_argument("--build-dir", required=True, type=Path)
    parser.add_argument("--qt-root", required=True, type=Path)
    arguments = parser.parse_args()
    arch = MACHINES[arguments.target][0]
    with zipfile.ZipFile(arguments.package) as package:
        files = package_files(package)

        def read(name: str) -> bytes:
            return package.read(files[name])

        if "AppxManifest.xml" not in files:
            fail("missing AppxManifest.xml")
        logos = verify_manifest(read("AppxManifest.xml"), (arguments.build_dir / "AppxManifest.xml").read_bytes(), arch)
        verify_payload(files, read, logos, arch)
        verify_imports(files, read, windows_library)
        verify_evidence(files, read, arguments.build_dir, arguments.qt_root)
    print(f"Windows package verified: {arguments.package.name}")


if __name__ == "__main__":
    main()
