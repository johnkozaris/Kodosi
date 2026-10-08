#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

from release_common import canonical_json, file_sha256


ROOT = Path(__file__).resolve().parents[2]
QT_MODULES = ("qtbase", "qtdeclarative", "qtsvg", "qtwayland")
SBOM_FORMATS = {
    "spdx": "spdx-tag-value",
    "spdx.json": "spdx-json",
    "cdx.json": "cyclonedx-json",
}
REQUIRED_QT_LICENSES = {
    "GPL-2.0-only.txt",
    "GPL-3.0-only.txt",
    "LGPL-3.0-only.txt",
    "Qt-GPL-exception-1.0.txt",
}


def fail(message: str) -> None:
    raise SystemExit(f"native license evidence error: {message}")


def load_json(path: Path) -> tuple[dict, bytes]:
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path} must contain a JSON object")
    return value, payload


def require_relative_file(root: Path, relative: str) -> Path:
    candidate = Path(relative)
    if (
        candidate.is_absolute()
        or ".." in candidate.parts
        or candidate.as_posix() != relative
    ):
        fail(f"unsafe evidence path: {relative}")
    path = root / candidate
    if path.is_symlink() or not path.is_file():
        fail(f"evidence is not a regular file: {relative}")
    return path


def read_soname(path: Path) -> str:
    result = subprocess.run(
        ["readelf", "-d", str(path)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"cannot inspect ELF metadata for {path.name}: {result.stderr.strip()}")
    match = re.search(r"\(SONAME\).*\[([^]]+)\]", result.stdout)
    if not match:
        fail(f"ELF library has no SONAME: {path.name}")
    return match.group(1)


def verify_committed_files(license_root: Path, manifest: dict) -> None:
    rows = manifest.get("committedFiles")
    if not isinstance(rows, list) or not rows:
        fail("committed evidence file list is malformed")
    expected: set[str] = set()
    for row in rows:
        if (
            not isinstance(row, dict)
            or set(row) != {"path", "sha256", "source"}
            or not isinstance(row["path"], str)
            or not isinstance(row["sha256"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
            or row["source"] not in {*QT_MODULES, "icu"}
        ):
            fail("committed evidence row is malformed")
        if row["path"] in expected:
            fail(f"duplicate committed evidence path: {row['path']}")
        expected.add(row["path"])
        path = require_relative_file(license_root, row["path"])
        if file_sha256(path) != row["sha256"]:
            fail(f"committed evidence digest changed: {row['path']}")
    actual = {
        path.relative_to(license_root).as_posix()
        for base in (
            license_root / "qt" / "6.11.2",
            license_root / "icu" / "73.2",
        )
        for path in base.rglob("*")
        if path.is_file() or path.is_symlink()
    }
    if actual != expected:
        fail("committed native license evidence file set changed")
    qt_licenses = {
        Path(path).name
        for path in expected
        if path.startswith("qt/6.11.2/LICENSES/")
    }
    if not REQUIRED_QT_LICENSES.issubset(qt_licenses):
        fail("required Qt LGPL/GPL license texts are missing")
    if any("LicenseRef-Qt-Commercial" in path for path in expected):
        fail("commercial Qt license text must not be redistributed as entitlement")
    icu_names = {
        Path(path).name for path in expected if path.startswith("icu/73.2/")
    }
    if not {"LICENSE", "license.html", "README.txt"}.issubset(icu_names):
        fail("ICU license and copyright evidence is incomplete")


def verify_source_archives(manifest: dict) -> None:
    expected = {
        "qtbase": (
            "https://download.qt.io/official_releases/qt/6.11/6.11.2/"
            "submodules/qtbase-everywhere-src-6.11.2.tar.xz",
            "v6.11.2",
            "5b2e00eccaf5a4d8c14134ffa0ea8dfd0a35ae1ffc7f8d87fa4305a1ed23cf22",
        ),
        "qtdeclarative": (
            "https://download.qt.io/official_releases/qt/6.11/6.11.2/"
            "submodules/qtdeclarative-everywhere-src-6.11.2.tar.xz",
            "v6.11.2",
            "215b7b70517e380123eabc6b92243f3c47b6f016a91d126057dbe53551c6b430",
        ),
        "qtsvg": (
            "https://download.qt.io/official_releases/qt/6.11/6.11.2/"
            "submodules/qtsvg-everywhere-src-6.11.2.tar.xz",
            "v6.11.2",
            "d594337feca84c26fb67fe87b85e6a5c12fda404b611d905f9d138210c311876",
        ),
        "qtwayland": (
            "https://download.qt.io/official_releases/qt/6.11/6.11.2/"
            "submodules/qtwayland-everywhere-src-6.11.2.tar.xz",
            "v6.11.2",
            "8eb7615e39332a10f506e8dd70f02d5954bb5949ff54f6dcbf8bd6168222f9df",
        ),
        "icu": (
            "https://github.com/unicode-org/icu/releases/download/"
            "release-73-2/icu4c-73_2-src.tgz",
            "release-73-2",
            "818a80712ed3caacd9b652305e01afc7fa167e6f2e94996da44b90c2ab604ce1",
        ),
    }
    rows = manifest.get("sourceArchives")
    if not isinstance(rows, list) or len(rows) != len(expected):
        fail("source archive evidence is malformed")
    measured: dict[str, tuple[str, str, str]] = {}
    for row in rows:
        if (
            not isinstance(row, dict)
            or set(row) != {"name", "url", "ref", "sha256"}
            or not isinstance(row["name"], str)
            or not isinstance(row["url"], str)
            or not isinstance(row["ref"], str)
            or not isinstance(row["sha256"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
        ):
            fail("source archive evidence row is malformed")
        measured[row["name"]] = (row["url"], row["ref"], row["sha256"])
    if measured != expected:
        fail("native license source archive metadata changed")


def verify_sboms(qt_root: Path, manifest: dict, installed_root: Path | None) -> None:
    qt = manifest.get("qt")
    if (
        not isinstance(qt, dict)
        or set(qt) != {
            "version",
            "binaryDistribution",
            "modules",
            "sboms",
        }
        or qt["version"] != "6.11.2"
        or qt["binaryDistribution"] != "linux_gcc_64"
        or qt["modules"] != list(QT_MODULES)
        or not isinstance(qt["sboms"], list)
    ):
        fail("Qt evidence identity is malformed")
    expected_names = {
        f"{module}-6.11.2.{suffix}"
        for module in QT_MODULES
        for suffix in SBOM_FORMATS
    }
    actual_names: set[str] = set()
    for row in qt["sboms"]:
        if (
            not isinstance(row, dict)
            or set(row) != {"filename", "format", "module", "sha256"}
            or row["module"] not in QT_MODULES
            or not isinstance(row["filename"], str)
            or not isinstance(row["sha256"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])
        ):
            fail("Qt SBOM evidence row is malformed")
        suffix = row["filename"].removeprefix(f"{row['module']}-6.11.2.")
        if SBOM_FORMATS.get(suffix) != row["format"]:
            fail(f"Qt SBOM format is inconsistent: {row['filename']}")
        actual_names.add(row["filename"])
        source = require_relative_file(qt_root, f"sbom/{row['filename']}")
        if file_sha256(source) != row["sha256"]:
            fail(f"pinned Qt binary SBOM digest changed: {row['filename']}")
        if installed_root is not None:
            packaged = require_relative_file(
                installed_root,
                f"usr/share/doc/kodosi/qt/sbom/{row['filename']}",
            )
            if file_sha256(packaged) != row["sha256"]:
                fail(f"packaged Qt SBOM digest changed: {row['filename']}")
    if actual_names != expected_names:
        fail("Qt SBOM file set is incomplete")


def verify_packaged_evidence(
    license_root: Path,
    manifest_path: Path,
    manifest: dict,
    installed_root: Path,
) -> None:
    packaged_manifest = require_relative_file(
        installed_root,
        "usr/share/doc/kodosi/provenance/native-license-evidence.json",
    )
    if packaged_manifest.read_bytes() != manifest_path.read_bytes():
        fail("packaged native evidence manifest differs")
    for row in manifest["committedFiles"]:
        source = license_root / row["path"]
        relative = Path(row["path"])
        if relative.parts[:2] == ("qt", "6.11.2"):
            destination = Path("usr/share/doc/kodosi/qt").joinpath(
                *relative.parts[2:]
            )
        elif relative.parts[:2] == ("icu", "73.2"):
            destination = Path("usr/share/doc/kodosi/icu").joinpath(
                *relative.parts[2:]
            )
        else:
            fail(f"unsupported packaged evidence path: {row['path']}")
        packaged = require_relative_file(installed_root, destination.as_posix())
        if packaged.read_bytes() != source.read_bytes():
            fail(f"packaged native evidence differs: {row['path']}")


def verify_runtime(installed_root: Path, manifest: dict) -> None:
    runtime = manifest.get("deployedRuntime")
    if (
        not isinstance(runtime, dict)
        or set(runtime)
        != {"qtLibraries", "icuLibraries", "plugins", "qmlPlugins"}
    ):
        fail("deployed runtime evidence is malformed")
    private_root = installed_root / "usr/lib/kodosi"
    for key, pattern in (
        ("plugins", "plugins/**/*.so"),
        ("qmlPlugins", "qml/**/*.so"),
    ):
        expected = runtime[key]
        if not isinstance(expected, list) or not all(
            isinstance(path, str) for path in expected
        ):
            fail(f"deployed {key} evidence is malformed")
        actual = sorted(
            path.relative_to(private_root).as_posix()
            for path in private_root.glob(pattern)
            if path.is_file()
        )
        if actual != expected:
            fail(f"deployed {key} set differs from native evidence")
    for key, prefix, version in (
        ("qtLibraries", "libQt6", "6.11.2"),
        ("icuLibraries", "libicu", "73.2"),
    ):
        rows = runtime[key]
        if not isinstance(rows, list):
            fail(f"deployed {key} evidence is malformed")
        expected: dict[str, str] = {}
        modules: set[str] = set()
        for row in rows:
            required = {"filename", "soname"}
            if key == "qtLibraries":
                required.add("module")
            if not isinstance(row, dict) or set(row) != required:
                fail(f"deployed {key} row is malformed")
            if key == "qtLibraries":
                if row["module"] not in QT_MODULES:
                    fail("deployed Qt module evidence is malformed")
                modules.add(row["module"])
            expected[row["filename"]] = row["soname"]
        actual_paths = sorted(
            path
            for path in (private_root / "lib").glob(f"{prefix}*.so.{version}")
            if path.is_file()
        )
        actual = {path.name: read_soname(path) for path in actual_paths}
        if actual != expected:
            fail(f"deployed {key} SONAME/version set differs from native evidence")
        if key == "qtLibraries" and modules != set(QT_MODULES):
            fail("deployed Qt module evidence is incomplete")
    icu = manifest.get("icu")
    if not isinstance(icu, dict) or icu != {"version": "73.2"}:
        fail("ICU evidence identity is malformed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest",
        type=Path,
        default=ROOT / "packaging/licenses/native-license-evidence.json",
    )
    parser.add_argument(
        "--license-root",
        type=Path,
        default=ROOT / "packaging/licenses",
    )
    parser.add_argument(
        "--qt-root",
        type=Path,
        default=ROOT / ".tools/Qt/6.11.2/gcc_64",
    )
    parser.add_argument("--dependencies", type=Path, default=ROOT / "dependencies.lock.json")
    parser.add_argument("--installed-root", type=Path)
    arguments = parser.parse_args()

    manifest, payload = load_json(arguments.manifest)
    if payload != canonical_json(manifest):
        fail("native evidence manifest is not canonical JSON")
    if manifest.get("schemaVersion") != 1 or set(manifest) != {
        "schemaVersion",
        "qt",
        "icu",
        "sourceArchives",
        "committedFiles",
        "deployedRuntime",
    }:
        fail("native evidence manifest schema is unsupported")
    dependencies, _ = load_json(arguments.dependencies)
    try:
        qt_version = dependencies["frameworks"]["qt"]["version"]
    except (KeyError, TypeError) as error:
        fail(f"Qt dependency pin is malformed: {error}")
    if qt_version != manifest["qt"]["version"]:
        fail("native evidence Qt version differs from dependencies.lock.json")

    verify_source_archives(manifest)
    verify_committed_files(arguments.license_root.resolve(strict=True), manifest)
    installed_root = (
        arguments.installed_root.resolve(strict=True)
        if arguments.installed_root is not None
        else None
    )
    verify_sboms(arguments.qt_root.resolve(strict=True), manifest, installed_root)
    if installed_root is not None:
        verify_packaged_evidence(
            arguments.license_root.resolve(strict=True),
            arguments.manifest.resolve(strict=True),
            manifest,
            installed_root,
        )
        verify_runtime(installed_root, manifest)
    print("Qt 6.11.2 and ICU 73.2 native license/SBOM evidence verified")


if __name__ == "__main__":
    main()
