#!/usr/bin/env python3
"""Builds Kodosi Browser: the engine in browser/engine on the CEF build that its `cef` crate pins.
Nothing compiles Chromium; CEF comes ready-built and checked. A Mac build is a background app
with no window, links or files of its own, signed and notarized when the signing variables are
set. Leaves the program in <output-dir>/<platform>/ and writes
kodosi-browser-<version>-<platform>.tar.gz with its feed entry to <output-dir>."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ENGINE = ROOT / "browser/engine"
TOOLS = ROOT / ".tools/browser-engine"
CEF_CACHE = ROOT / ".tools/cef"
# The CEF wrapper builds with Ninja and CMake, from these releases on the Python package index.
BUILD_TOOLS = ("ninja==1.13.0", "cmake==4.1.2")
APP_ID = "com.kodosi.browser"
PRODUCT = "Kodosi Browser"
HELPERS = {
    "": "helper",
    " (GPU)": "helper.gpu",
    " (Renderer)": "helper.renderer",
    " (Plugin)": "helper.plugin",
    " (Alerts)": "helper.alerts",
}
JIT = {"com.apple.security.cs.allow-jit": True}
ENTITLEMENTS = {
    " (GPU)": JIT,
    " (Renderer)": JIT,
    " (Plugin)": {
        **JIT,
        "com.apple.security.cs.allow-unsigned-executable-memory": True,
        "com.apple.security.cs.disable-library-validation": True,
    },
}
MINIMUM_MAC = "12.0"
# The parts of a CEF download that only a build uses. The SUID sandbox helper works only when
# root owns it, which a download never is.
CEF_BUILD_PARTS = {"include", "cmake", "libcef_dll", "CMakeLists.txt", "archive.json", "chrome-sandbox"}


def platform_name() -> str:
    system = {"Darwin": "mac", "Linux": "linux", "Windows": "windows"}[platform.system()]
    machine = {"arm64": "arm64", "aarch64": "arm64"}.get(platform.machine().lower(), "x64")
    return f"{system}-{machine}"


def run(*command: str, folder: Path = ROOT, env: dict[str, str] | None = None) -> None:
    subprocess.run(command, cwd=folder, env=env, check=True, stdout=sys.stderr)


def build_tools() -> Path:
    """A private Python environment with the pinned Ninja and CMake, made by uv as for the Qt
    tools."""
    venv = TOOLS / "venv"
    windows = platform.system() == "Windows"
    binaries = venv / ("Scripts" if windows else "bin")
    stamp = venv / "tools.txt"
    wanted = "\n".join(BUILD_TOOLS)
    if not stamp.is_file() or stamp.read_text("utf-8") != wanted:
        uv = shutil.which("uv")
        if uv is None:
            raise SystemExit("uv is needed for the build tools of Kodosi Browser")
        run(uv, "venv", "--clear", "--quiet", str(venv))
        python = binaries / ("python.exe" if windows else "python")
        run(uv, "pip", "install", "--quiet", "--python", str(python), *BUILD_TOOLS)
        stamp.write_text(wanted, "utf-8")
    return binaries


def environment() -> dict[str, str]:
    return {
        **os.environ,
        "PATH": f"{build_tools()}{os.pathsep}{os.environ['PATH']}",
        "CEF_PATH": str(CEF_CACHE),
    }


def check() -> None:
    """Formats, lints and tests the engine, with warnings as errors."""
    manifest = ["--manifest-path", str(ENGINE / "Cargo.toml")]
    env = environment()
    run("cargo", "fmt", *manifest, "--", "--check", env=env)
    run("cargo", "clippy", "--locked", "--all-targets", *manifest, "--", "-D", "warnings", "-A", "clippy::unwrap_used", "-A", "clippy::expect_used", env=env)
    run("cargo", "test", "--locked", *manifest, env=env)


def build(debug: bool) -> tuple[Path, Path]:
    """Builds the engine and returns its target folder and the CEF that it uses."""
    environment_of_build = environment()
    profile = [] if debug else ["--release"]
    run("cargo", "build", "--locked", "--bins", *profile, "--manifest-path", str(ENGINE / "Cargo.toml"), env=environment_of_build)
    target = ENGINE / "target" / ("debug" if debug else "release")
    found = sorted(CEF_CACHE.glob("*/cef_*/archive.json"), key=lambda path: path.stat().st_mtime)
    if not found:
        raise SystemExit(f"No CEF in {CEF_CACHE}")
    return target, found[-1].parent


def version_of(cef: Path) -> str:
    """The Chromium version and the engine's version, as the feed names a release."""
    name = json.loads((cef / "archive.json").read_text("utf-8"))["name"]
    chromium = name.split("+chromium-", 1)[1].split("_", 1)[0]
    manifest = (ENGINE / "Cargo.toml").read_text("utf-8")
    engine = next(line.split('"')[1] for line in manifest.splitlines() if line.startswith("version = "))
    return f"{chromium}-{engine}"


def info(path: Path, executable: str, bundle_id: str, version: str) -> None:
    with path.open("wb") as file:
        plistlib.dump(
            {
                "CFBundleExecutable": executable,
                "CFBundleIdentifier": bundle_id,
                "CFBundleName": executable,
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "LSUIElement": True,
                "LSMinimumSystemVersion": MINIMUM_MAC,
                "NSHighResolutionCapable": True,
                "NSSupportsAutomaticGraphicsSwitching": True,
            },
            file,
        )


def mac_app(target: Path, cef: Path, stage: Path, version: str) -> Path:
    app = stage / f"{PRODUCT}.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    frameworks = contents / "Frameworks"
    frameworks.mkdir()
    shutil.copy2(target / "kodosi-browser", contents / "MacOS" / PRODUCT)
    info(contents / "Info.plist", PRODUCT, APP_ID, version)
    shutil.copy2(cef / "CREDITS.html", contents / "Resources" / "CREDITS.html")
    shutil.copytree(cef / "Chromium Embedded Framework.framework", frameworks / "Chromium Embedded Framework.framework", symlinks=True)
    for kind, suffix in HELPERS.items():
        name = f"{PRODUCT} Helper{kind}"
        helper = frameworks / f"{name}.app" / "Contents"
        (helper / "MacOS").mkdir(parents=True)
        shutil.copy2(target / "kodosi-browser-helper", helper / "MacOS" / name)
        info(helper / "Info.plist", name, f"{APP_ID}.{suffix}", version)
    sign_mac(app, stage)
    notarize_mac(app, stage)
    return app


def sign_mac(app: Path, stage: Path) -> None:
    """Signs from the inside out: libraries, the framework, each helper with its entitlements,
    then the app."""
    identity = os.environ.get("KODOSI_SIGNING_IDENTITY")
    if not identity:
        return
    frameworks = app / "Contents/Frameworks"
    framework = frameworks / "Chromium Embedded Framework.framework"
    sign = ["codesign", "--force", "--timestamp", "--options", "runtime", "--sign", identity]
    for library in sorted((framework / "Libraries").glob("*.dylib")):
        run(*sign, str(library))
    run(*sign, str(framework))
    for kind in HELPERS:
        helper = frameworks / f"{PRODUCT} Helper{kind}.app"
        entitlements = ENTITLEMENTS.get(kind)
        if entitlements:
            plist = stage / f"entitlements{kind.strip(' ()').lower() or '-helper'}.plist"
            with plist.open("wb") as file:
                plistlib.dump(entitlements, file)
            run(*sign, "--entitlements", str(plist), str(helper))
            plist.unlink()
        else:
            run(*sign, str(helper))
    run(*sign, str(app))
    run("codesign", "--verify", "--deep", "--strict", str(app))


def notarize_mac(app: Path, stage: Path) -> None:
    profile = os.environ.get("KODOSI_NOTARY_PROFILE")
    if not profile:
        return
    archive = stage / "notarize.zip"
    run("ditto", "-c", "-k", "--keepParent", str(app), str(archive))
    run("xcrun", "notarytool", "submit", str(archive), "--keychain-profile", profile, "--wait")
    archive.unlink()
    run("xcrun", "stapler", "staple", str(app))


def plain_parts(target: Path, cef: Path, stage: Path, program: str) -> Path:
    """The program beside CEF's libraries and resources, which CEF finds next to it."""
    folder = stage / "kodosi-browser"
    folder.mkdir()
    for part in cef.iterdir():
        if part.name in CEF_BUILD_PARTS or part.suffix in (".lib", ".bz2"):
            continue
        if part.is_dir():
            shutil.copytree(part, folder / part.name)
        else:
            shutil.copy2(part, folder / part.name)
    built = target / ("kodosi-browser.exe" if program.endswith(".exe") else "kodosi-browser")
    shutil.copy2(built, folder / program)
    if platform.system() == "Linux":
        run("strip", "--strip-unneeded", program, "libcef.so", folder=folder)
    return folder / program


def package(stage: Path, output: Path, version: str, program: str) -> Path:
    name = platform_name()
    archive = output / f"kodosi-browser-{version}-{name}.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for path in sorted(stage.iterdir()):
            tar.add(path, arcname=path.name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    entry = {"version": version, "platform": name, "file": archive.name, "program": program, "sha256": digest}
    (output / f"{archive.name}.json").write_text(json.dumps(entry, indent=2) + "\n", "utf-8")
    return archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--debug", action="store_true", help="a quick build for development, not for release")
    parser.add_argument("--check", action="store_true", help="format, lint and test the engine, and build nothing")
    arguments = parser.parse_args()
    if arguments.check:
        check()
        return
    if arguments.output_dir is None:
        parser.error("--output-dir is needed for a build")
    target, cef = build(arguments.debug)
    version = version_of(cef)
    output = arguments.output_dir.resolve()
    stage = output / platform_name()
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    name = platform_name()
    if name.startswith("mac"):
        program = mac_app(target, cef, stage, version).name
    elif name.startswith("windows"):
        program = plain_parts(target, cef, stage, "kodosi-browser.exe").relative_to(stage).as_posix()
    else:
        program = plain_parts(target, cef, stage, "kodosi-browser").relative_to(stage).as_posix()
    print(package(stage, output, version, program))
    print(stage / program)


if __name__ == "__main__":
    main()
