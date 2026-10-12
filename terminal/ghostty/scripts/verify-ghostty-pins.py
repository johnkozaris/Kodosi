#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

sys.dont_write_bytecode = True

import digest  # noqa: E402

PLATFORMS = ("macos", "linux", "windows")
PLATFORM_NAMES = {"macos": "macOS and iOS", "linux": "Linux", "windows": "Windows"}
REFS = {"macos": "MacOSGhostty.ref", "linux": "LinuxGhostty.ref", "windows": "WindowsGhostty.ref"}
ZIG_TARGETS = {
    "macos": ("aarch64-macos",),
    "linux": ("x86_64-linux",),
    "windows": ("x86_64-windows", "aarch64-windows"),
}
MAC_SLICE = "macos-arm64"
NATIVE_BUILD_POLICY = {
    "GHOSTTY_BUILD_CPU": "baseline",
    "GHOSTTY_BUILD_OPTIMIZE": "ReleaseFast",
    "GHOSTTY_BUILD_EMIT_COMBINED_LIB": "true",
    "GHOSTTY_BUILD_APP_RUNTIME": "none",
}
BUILD_INPUTS = ("patches_sha256", "patch_application_sha256", "zig_store_sha256")
COMMON_KEYS = (
    "package_version",
    "ghostty_commit",
    "patches_sha256",
    "patch_application_sha256",
    "zig_version",
    "zig_target",
    "zig_sha256",
    "zig_store_sha256",
    "ghostty_build_options",
    "vt_target",
    "vt_cpu",
    "vt_optimize",
)
APPLE_KEYS = (
    *COMMON_KEYS,
    "metal_toolchain_build",
    "metal_toolchain_dmg_sha256",
    "combined_archive_sha256",
    "renderer_target",
    "renderer_cpu",
    "renderer_optimize",
    "symbol_manifest_sha256",
)
MAC_KEYS = (*APPLE_KEYS, "renderer_artifact_sha256", "vt_artifact_sha256")
IOS_KEYS = (*APPLE_KEYS, "library_identifier", "slice_artifact_sha256")
LINUX_KEYS = (*COMMON_KEYS, "vt_archive_sha256", "vt_artifact_sha256")
WINDOWS_KEYS = (*COMMON_KEYS, "vt_archive_sha256")
KEY = re.compile(r"[A-Za-z][A-Za-z0-9_]*")
VALUE = re.compile(r"[A-Za-z0-9._,=+-]*")
COMMIT = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"[0-9a-f]{64}")


def fail(message: str) -> None:
    raise SystemExit(f"[!] Ghostty pins: {message}")


def repository_root() -> Path:
    return Path(__file__).resolve().parents[3]


def label(path: Path) -> str:
    try:
        return path.resolve().relative_to(repository_root()).as_posix()
    except ValueError:
        return str(path)


def read_record(path: Path) -> str:
    try:
        payload = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {label(path)}: {error.strerror}")
    if b"\r" in payload:
        fail(f"{label(path)} has a carriage return. Use LF line ends.")
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError:
        fail(f"{label(path)} is not UTF-8 text")
    if not text.endswith("\n"):
        fail(f"{label(path)} does not end with a line feed")
    return text


class Record:
    def __init__(self, path: Path, values: dict[str, str]) -> None:
        self.label = label(path)
        self.values = values

    def value(self, key: str) -> str:
        if key not in self.values:
            fail(f"{self.label} has no {key}")
        return self.values[key]

    def require_keys(self, keys: tuple[str, ...]) -> None:
        missing = [key for key in keys if key not in self.values]
        if missing:
            fail(f"{self.label} has no {', '.join(missing)}")
        unknown = sorted(set(self.values) - set(keys))
        if unknown:
            fail(f"{self.label} has an unknown key: {', '.join(unknown)}")
        for key in keys:
            if key.endswith("_sha256") and not SHA256.fullmatch(self.values[key]):
                fail(f"{self.label} has {key}={self.values[key]}, which is not a SHA-256 digest")

    def expect(self, key: str, expected: str, source: str) -> None:
        actual = self.value(key)
        if actual != expected:
            fail(f"{self.label} has {key}={actual}, but {source} is {expected}")


def load_metadata(path: Path) -> Record:
    values: dict[str, str] = {}
    for number, line in enumerate(read_record(path)[:-1].split("\n"), start=1):
        key, separator, value = line.partition("=")
        if not separator or not KEY.fullmatch(key) or not VALUE.fullmatch(value):
            fail(f"line {number} of {label(path)} is not a key=value line")
        if key in values:
            fail(f"{label(path)} has {key} more than once")
        values[key] = value
    return Record(path, values)


def load_shell_values(path: Path) -> tuple[Record, str]:
    text = read_record(path)
    values = dict(re.findall(r"^([A-Z][A-Z0-9_]*)=([A-Za-z0-9._,=+:/-]*)$", text, re.MULTILINE))
    return Record(path, values), text


def load_ref(path: Path) -> str:
    text = read_record(path)
    if not COMMIT.fullmatch(text[:-1]):
        fail(f"{label(path)} must contain one lowercase 40-character commit and a line feed")
    return text[:-1]


def measure_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"{label(path)} is not a regular file")
    return digest.file_sha256(path)


def measure_folder(path: Path) -> str:
    if path.is_symlink() or not path.is_dir():
        fail(f"{label(path)} is not a folder")
    for relative in digest.artifact_paths(path):
        if any(part.startswith(".") for part in relative.split("/")):
            fail(f"{label(path / relative)} is not part of the artifact. Delete it.")
    return digest.artifact_sha256(path)


def lock_key(artifact: str) -> str:
    return f"{artifact.replace('-', '_')}_archive_sha256"


def apple_slices(native: Record) -> dict[str, str]:
    names = native.values.get("GHOSTTY_BUILD_APPLE_SLICES", "").split(",")
    if MAC_SLICE not in names or "" in names or len(names) != len(set(names)):
        fail(f"GHOSTTY_BUILD_APPLE_SLICES in {native.label} must list each Apple slice once, with {MAC_SLICE}")
    targets = {}
    for name in names:
        match = re.fullmatch(r"([a-z]+)-arm64(-simulator)?", name)
        platform = match.group(1).upper() if match else ""
        target = native.values.get(f"GHOSTTY_BUILD_{platform}_TARGET", "")
        i18n = native.values.get(f"GHOSTTY_BUILD_{platform}_I18N")
        if not match or not target or i18n not in {"true", "false"}:
            fail(f"{native.label} has no complete build values for the Apple slice {name}")
        targets[name] = target + (match.group(2) or "")
    for key, value in NATIVE_BUILD_POLICY.items():
        if native.values.get(key) != value:
            fail(f"{native.label} must have {key}={value}")
    return {MAC_SLICE: targets.pop(MAC_SLICE), **targets}


def windows_layouts(ghostty: Path) -> tuple[Record, dict[str, str]]:
    build, text = load_shell_values(ghostty / "WindowsVtBuild.env")
    pairs = re.findall(
        r"^\s+GHOSTTY_WINDOWS_VT_TARGET=(\S+)\n\s+GHOSTTY_WINDOWS_VT_LAYOUT=(\S+)$",
        text,
        re.MULTILINE,
    )
    if not pairs:
        fail(f"{build.label} names no Windows layout")
    return build, {layout: target for target, layout in pairs}


def vendored(vendor: Path, layout: str) -> bool:
    return (vendor / "GhosttyVt" / layout).exists() or (vendor / "GhosttyVt" / f"{layout}.version").exists()


class Verifier:
    def __init__(self, arguments: argparse.Namespace) -> None:
        self.ghostty = Path(os.environ.get("KODOSI_GHOSTTY_DIR") or Path(__file__).resolve().parents[1])
        self.vendor = arguments.vendor or self.ghostty / "Vendor"
        self.platforms = [arguments.platform] if arguments.platform else list(PLATFORMS)
        self.patched_source = arguments.patched_source
        self.version_path = self.ghostty / "VERSION"
        self.version = read_record(self.version_path)[:-1]
        if not VALUE.fullmatch(self.version) or not self.version:
            fail(f"{label(self.version_path)} must contain one version and a line feed")
        self.toolchain, _ = load_shell_values(self.ghostty / "Toolchain.env")
        self.native = load_metadata(self.ghostty / "NativeBuild.env")
        self.slices = apple_slices(self.native)
        self.linux = load_metadata(self.ghostty / "LinuxVtBuild.env")
        self.linux_layout = self.linux.value("GHOSTTY_LINUX_VT_LAYOUT")
        self.windows, self.windows_targets = windows_layouts(self.ghostty)
        self.lock = None if arguments.working_tree else self.load_lock()

    def load_lock(self) -> Record:
        lock = load_metadata(repository_root() / "Ghostty.lock")
        artifacts = {
            "macos": list(self.slices),
            "linux": [self.linux_layout],
            "windows": [layout for layout in self.windows_targets if vendored(self.vendor, layout)],
        }
        known = {f"{platform}_ghostty_commit" for platform in PLATFORMS}
        known.update(lock_key(name) for name in [*self.slices, self.linux_layout, *self.windows_targets])
        unknown = sorted(set(lock.values) - known)
        if unknown:
            fail(f"{lock.label} has an unknown key: {', '.join(unknown)}")
        for platform in self.platforms:
            lock.value(f"{platform}_ghostty_commit")
            for name in artifacts[platform]:
                lock.value(lock_key(name))
        if "windows" in self.platforms:
            for layout in self.windows_targets:
                if lock_key(layout) in lock.values and not vendored(self.vendor, layout):
                    fail(
                        f"{lock.label} has {lock_key(layout)}, but "
                        f"{label(self.vendor / 'GhosttyVt' / layout)} does not exist"
                    )
        for key, value in lock.values.items():
            if key.endswith("_ghostty_commit") and not COMMIT.fullmatch(value):
                fail(f"{lock.label} has {key}={value}, which is not a lowercase 40-character commit")
            if key.endswith("_archive_sha256") and not SHA256.fullmatch(value):
                fail(f"{lock.label} has {key}={value}, which is not a SHA-256 digest")
        return lock

    def verify_commit(self, platform: str) -> str:
        path = self.ghostty / REFS[platform]
        commit = load_ref(path)
        if self.lock is not None:
            self.lock.expect(f"{platform}_ghostty_commit", commit, f"the commit in {label(path)}")
        return commit

    def verify_archive(self, record: Record, key: str, artifact: str, path: Path) -> str:
        measured = measure_file(path)
        if self.lock is not None:
            self.lock.expect(lock_key(artifact), measured, f"the SHA-256 of {label(path)}")
        record.expect(key, measured, f"the SHA-256 of {label(path)}")
        return measured

    def verify_identity(self, record: Record, platform: str, commit: str) -> None:
        record.expect("package_version", self.version, f"the version in {label(self.version_path)}")
        record.expect("ghostty_commit", commit, f"the commit in {label(self.ghostty / REFS[platform])}")
        zig_target = record.value("zig_target")
        if zig_target not in ZIG_TARGETS[platform]:
            fail(
                f"{record.label} has zig_target={zig_target}, but the {PLATFORM_NAMES[platform]} "
                f"build uses {' or '.join(ZIG_TARGETS[platform])}"
            )
        record.expect("zig_version", self.toolchain.value("ZIG_VERSION"), f"ZIG_VERSION in {self.toolchain.label}")
        zig_key = f"ZIG_{zig_target.upper().replace('-', '_')}_SHA256"
        record.expect("zig_sha256", self.toolchain.value(zig_key), f"{zig_key} in {self.toolchain.label}")

    def verify_build_values(self, record: Record, build: Record, prefixes: tuple[str, ...], values: dict[str, str]) -> None:
        for prefix in prefixes:
            for suffix, key in values.items():
                record.expect(f"{prefix}_{suffix}", build.value(key), f"{key} in {build.label}")

    def build_inputs(self, commit: str) -> dict[str, str]:
        source = self.patched_source
        if not (source / ".git").is_dir() or not (source / "zig-pkg").is_dir():
            fail(f"{label(source)} is not a patched Ghostty source with a Zig package store")
        return {
            "patches_sha256": measure_folder(self.ghostty / "Patches" / "ghostty"),
            "patch_application_sha256": digest.patch_application_sha256(source, commit),
            "zig_store_sha256": digest.tree_sha256(source / "zig-pkg"),
        }

    def expect_build_inputs(self, record: Record, inputs: dict[str, str], options_script: str, *options: str) -> None:
        sources = {
            "patches_sha256": f"the digest of {label(self.ghostty / 'Patches' / 'ghostty')}",
            "patch_application_sha256": f"the patch application digest of {label(self.patched_source)}",
            "zig_store_sha256": f"the digest of {label(self.patched_source / 'zig-pkg')}",
        }
        for key, value in inputs.items():
            record.expect(key, value, sources[key])
        script = self.ghostty / "scripts" / options_script
        bash = shutil.which("bash")
        if bash is None:
            fail("bash is required to read the build options")
        result = subprocess.run(
            [bash, str(script), "vector", *options],
            stdout=subprocess.PIPE,
            universal_newlines=True,
            check=False,
        )
        if result.returncode != 0:
            fail(f"{label(script)} failed")
        command = " ".join([label(script), "vector", *options])
        record.expect("ghostty_build_options", result.stdout.strip(), f"the output of {command}")

    def verify_apple(self) -> str:
        commit = self.verify_commit("macos")
        xcframework = self.vendor / "GhosttyKit.xcframework"
        try:
            folders = {path.name for path in xcframework.iterdir() if path.is_dir()}
        except OSError as error:
            fail(f"cannot read {label(xcframework)}: {error.strerror}")
        if folders != set(self.slices):
            missing = sorted(set(self.slices) - folders)
            extra = sorted(folders - set(self.slices))
            fail(
                f"the slices in {label(xcframework)} differ from GHOSTTY_BUILD_APPLE_SLICES in "
                f"{self.native.label}: missing {', '.join(missing) or 'none'}, extra {', '.join(extra) or 'none'}"
            )
        build_values = {"cpu": "GHOSTTY_BUILD_CPU", "optimize": "GHOSTTY_BUILD_OPTIMIZE"}
        records: dict[str, Record] = {}
        for name, target in self.slices.items():
            is_mac = name == MAC_SLICE
            record = load_metadata(self.vendor / ("libghostty.version" if is_mac else f"libghostty-{name}.version"))
            record.require_keys(MAC_KEYS if is_mac else IOS_KEYS)
            self.verify_identity(record, "macos", commit)
            for key in ("metal_toolchain_build", "metal_toolchain_dmg_sha256"):
                record.expect(key, self.toolchain.value(key.upper()), f"{key.upper()} in {self.toolchain.label}")
            for prefix in ("renderer", "vt"):
                record.expect(f"{prefix}_target", target, f"the target of {name} in {self.native.label}")
            self.verify_build_values(record, self.native, ("renderer", "vt"), build_values)
            if not is_mac:
                record.expect("library_identifier", name, "the name of its slice")
            records[name] = record
        mac = records[MAC_SLICE]
        archives = {
            name: self.verify_archive(record, "combined_archive_sha256", name, xcframework / name / "libghostty.a")
            for name, record in records.items()
        }
        vt = self.vendor / "GhosttyVt" / MAC_SLICE
        vt_archive = vt / "lib" / "libghostty-vt.a"
        if measure_file(vt_archive) != archives[MAC_SLICE]:
            fail(
                f"{label(vt_archive)} differs from {label(xcframework / MAC_SLICE / 'libghostty.a')}. "
                "They must be one native image."
            )
        for name, record in records.items():
            if name == MAC_SLICE:
                continue
            if archives[name] == archives[MAC_SLICE]:
                fail(
                    f"{label(xcframework / name / 'libghostty.a')} is the macOS image. "
                    "Each Apple slice needs its own image."
                )
            for key in BUILD_INPUTS:
                record.expect(key, mac.value(key), f"{key} in {mac.label}")
        symbols = self.vendor / "symbols"
        mac.expect("renderer_artifact_sha256", measure_folder(xcframework), f"the digest of {label(xcframework)}")
        mac.expect("vt_artifact_sha256", measure_folder(vt), f"the digest of {label(vt)}")
        mac.expect("symbol_manifest_sha256", measure_folder(symbols), f"the digest of {label(symbols)}")
        for name, record in records.items():
            if name == MAC_SLICE:
                continue
            folder = xcframework / name
            record.expect("slice_artifact_sha256", measure_folder(folder), f"the digest of {label(folder)}")
            manifest = symbols / f"{name}-strong.txt"
            record.expect("symbol_manifest_sha256", measure_file(manifest), f"the SHA-256 of {label(manifest)}")
        if self.patched_source is not None:
            inputs = self.build_inputs(commit)
            for name, record in records.items():
                self.expect_build_inputs(record, inputs, "native-build-options.sh", name)
        return commit

    def verify_linux(self) -> str:
        commit = self.verify_commit("linux")
        layout = self.linux_layout
        folder = self.vendor / "GhosttyVt" / layout
        record = load_metadata(self.vendor / "GhosttyVt" / f"{layout}.version")
        record.require_keys(LINUX_KEYS)
        self.verify_identity(record, "linux", commit)
        self.verify_build_values(
            record,
            self.linux,
            ("vt",),
            {"target": "GHOSTTY_LINUX_VT_TARGET", "cpu": "GHOSTTY_LINUX_VT_CPU", "optimize": "GHOSTTY_LINUX_VT_OPTIMIZE"},
        )
        self.verify_archive(record, "vt_archive_sha256", layout, folder / "lib" / "libghostty-vt.a")
        # Git for Windows writes symbolic links as plain files. Thus this folder has another digest there.
        if os.name != "nt":
            record.expect("vt_artifact_sha256", measure_folder(folder), f"the digest of {label(folder)}")
        if self.patched_source is not None:
            self.expect_build_inputs(record, self.build_inputs(commit), "linux-vt-build-options.sh")
        return commit

    def verify_windows(self) -> str:
        commit = self.verify_commit("windows")
        for layout, target in self.windows_targets.items():
            if not vendored(self.vendor, layout):
                continue
            folder = self.vendor / "GhosttyVt" / layout
            record = load_metadata(self.vendor / "GhosttyVt" / f"{layout}.version")
            record.require_keys(WINDOWS_KEYS)
            self.verify_identity(record, "windows", commit)
            record.expect("vt_target", target, f"the target of {layout} in {self.windows.label}")
            self.verify_build_values(
                record,
                self.windows,
                ("vt",),
                {"cpu": "GHOSTTY_WINDOWS_VT_CPU", "optimize": "GHOSTTY_WINDOWS_VT_OPTIMIZE"},
            )
            self.verify_archive(record, "vt_archive_sha256", layout, folder / "lib" / "ghostty-vt-static.lib")
        return commit

    def verify(self) -> None:
        verifiers = {"macos": self.verify_apple, "linux": self.verify_linux, "windows": self.verify_windows}
        commits = {platform: verifiers[platform]() for platform in self.platforms}
        if self.lock is None:
            print("[*] The check did not use Ghostty.lock. Do not publish this build.", file=sys.stderr)
        print(
            "[*] Verified the Ghostty pins: "
            + ", ".join(f"{PLATFORM_NAMES[platform]} {commit}" for platform, commit in commits.items())
        )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=(
            "Verify the Ghostty pins: Ghostty.lock, the platform ref files, NativeBuild.env, "
            "the metadata of the vendored artifacts and their digests."
        ),
        epilog=(
            "KODOSI_GHOSTTY_DIR selects the Ghostty folder. "
            "The default is the folder that contains this scripts folder."
        ),
    )
    parser.add_argument(
        "--working-tree",
        action="store_true",
        help="do not read Ghostty.lock. Use this for a build that you do not publish.",
    )
    parser.add_argument(
        "--vendor",
        type=Path,
        metavar="FOLDER",
        help="the folder of the vendored artifacts. The default is Vendor in the Ghostty folder.",
    )
    parser.add_argument(
        "--platform",
        choices=PLATFORMS,
        help="verify only the artifacts of this platform. macos includes the iOS slices.",
    )
    parser.add_argument(
        "--patched-source",
        type=Path,
        metavar="FOLDER",
        help=(
            "also compare the build inputs with this patched Ghostty source. "
            "Use it with --platform macos or --platform linux."
        ),
    )
    arguments = parser.parse_args()
    if arguments.patched_source is not None and arguments.platform not in {"macos", "linux"}:
        parser.error("--patched-source needs --platform macos or --platform linux")
    Verifier(arguments).verify()


if __name__ == "__main__":
    main()
