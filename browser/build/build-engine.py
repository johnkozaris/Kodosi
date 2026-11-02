#!/usr/bin/env python3
"""Builds Kodosi Browser: the Chromium that Chromium.lock pins, with H.264, a name and an app id
of its own, and no change to the engine. A Mac build is also a background app with no links or
files of its own, and it is signed and notarized when the signing variables are set. Writes
kodosi-browser-<version>-<platform>.tar.gz and its feed entry to <output-dir>."""

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
WORK = ROOT / ".tools/chromium"
DEPOT_TOOLS = "https://chromium.googlesource.com/chromium/tools/depot_tools.git"
CHROMIUM = "https://chromium.googlesource.com/chromium/src.git"
APP_ID = "com.kodosi.browser"
PRODUCT = "Kodosi Browser"
BRANDING = {
    "COMPANY_FULLNAME": "Kodosi",
    "COMPANY_SHORTNAME": "Kodosi",
    "PRODUCT_FULLNAME": PRODUCT,
    "PRODUCT_SHORTNAME": PRODUCT,
    "PRODUCT_INSTALLER_FULLNAME": PRODUCT,
    "PRODUCT_INSTALLER_SHORTNAME": PRODUCT,
    "MAC_BUNDLE_ID": APP_ID,
    "MAC_CREATOR_CODE": "Kdsi",
}
BUILD_ARGUMENTS = {
    "is_official_build": "true",
    "is_debug": "false",
    "is_component_build": "false",
    "symbol_level": "0",
    "blink_symbol_level": "0",
    "v8_symbol_level": "0",
    "proprietary_codecs": "true",
    "ffmpeg_branding": '"Chrome"',
    "rtc_use_h264": "true",
    "enable_widevine": "false",
    "treat_warnings_as_errors": "false",
}
NOT_FOR_THE_MAC_APP = (
    "CFBundleDocumentTypes",
    "CFBundleURLTypes",
    "NSUserActivityTypes",
    "UTExportedTypeDeclarations",
    "UTImportedTypeDeclarations",
)
LINUX_PARTS = (
    "chrome",
    "chrome_crashpad_handler",
    "chrome_sandbox",
    "icudtl.dat",
    "libEGL.so",
    "libGLESv2.so",
    "libvk_swiftshader.so",
    "libvulkan.so.1",
    "locales",
    "resources.pak",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "snapshot_blob.bin",
    "v8_context_snapshot.bin",
    "vk_swiftshader_icd.json",
)
WINDOWS_PARTS = (
    "chrome.exe",
    "chrome.dll",
    "chrome_elf.dll",
    "chrome_wer.dll",
    "dbghelp.dll",
    "d3dcompiler_47.dll",
    "dxcompiler.dll",
    "dxil.dll",
    "libEGL.dll",
    "libGLESv2.dll",
    "vk_swiftshader.dll",
    "vulkan-1.dll",
    "icudtl.dat",
    "locales",
    "resources.pak",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "snapshot_blob.bin",
    "v8_context_snapshot.bin",
    "vk_swiftshader_icd.json",
)


def pin() -> str:
    values = dict(
        line.split("=", 1) for line in (HERE / "Chromium.lock").read_text("utf-8").split()
    )
    if set(values) != {"version"} or values["version"].count(".") != 3:
        raise SystemExit("Chromium.lock names one version of Chromium")
    return values["version"]


def platform_name() -> str:
    system = {"Darwin": "mac", "Linux": "linux", "Windows": "windows"}[platform.system()]
    machine = {"arm64": "arm64", "aarch64": "arm64"}.get(platform.machine().lower(), "x64")
    return f"{system}-{machine}"


def environment() -> dict[str, str]:
    tools = str(WORK / "depot_tools")
    return {
        **os.environ,
        "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
        "DEPOT_TOOLS_UPDATE": "0",
        "DEPOT_TOOLS_WIN_TOOLCHAIN": "0",
    }


def run(*command: str, folder: Path) -> None:
    subprocess.run(command, cwd=folder, env=environment(), check=True, stdout=sys.stderr)


def tool(name: str) -> str:
    found = shutil.which(name, path=environment()["PATH"])
    if not found:
        raise SystemExit(f"{name} is missing")
    return found


def source(version: str) -> Path:
    tools = WORK / "depot_tools"
    if not tools.is_dir():
        WORK.mkdir(parents=True, exist_ok=True)
        run("git", "clone", "--quiet", "--depth", "1", DEPOT_TOOLS, str(tools), folder=WORK)
    first_run = "update_depot_tools.bat" if platform.system() == "Windows" else "ensure_bootstrap"
    run(str(tools / first_run), folder=tools)
    src = WORK / "src"
    if not src.is_dir():
        src.mkdir()
        run("git", "init", "--quiet", folder=src)
        run("git", "remote", "add", "origin", CHROMIUM, folder=src)
    (WORK / ".gclient").write_text(
        'solutions = [{"name": "src", "url": "' + CHROMIUM + '", "managed": False, '
        '"custom_vars": {"checkout_pgo_profiles": True}}]\n',
        "utf-8",
    )
    run("git", "fetch", "--depth", "1", "origin", f"refs/tags/{version}:refs/tags/{version}", folder=src)
    run("git", "-c", "advice.detachedHead=false", "checkout", "--quiet", "--force", f"refs/tags/{version}", folder=src)
    run(tool("gclient"), "sync", "--no-history", "--shallow", "--force", "--reset", folder=WORK)
    return src


def brand(src: Path) -> None:
    path = src / "chrome/app/theme/chromium/BRANDING"
    lines = []
    for line in path.read_text("utf-8").splitlines():
        name = line.partition("=")[0]
        lines.append(f"{name}={BRANDING[name]}" if name in BRANDING else line)
    path.write_text("\n".join(lines) + "\n", "utf-8")


def build(src: Path) -> Path:
    out = src / "out/Engine"
    out.mkdir(parents=True, exist_ok=True)
    arguments = "\n".join(f"{name} = {value}" for name, value in BUILD_ARGUMENTS.items())
    (out / "args.gn").write_text(arguments + "\n", "utf-8")
    run(tool("gn"), "gen", "out/Engine", folder=src)
    run(tool("autoninja"), "-C", "out/Engine", "chrome", folder=src)
    return out


def sign_mac(app: Path) -> None:
    identity = os.environ.get("KODOSI_SIGNING_IDENTITY")
    if not identity:
        return
    versions = app / "Contents/Frameworks" / f"{PRODUCT} Framework.framework/Versions"
    current = versions / "Current"
    insides = [
        *sorted((current / "Helpers").glob("*.app")),
        *sorted(path for path in (current / "Helpers").iterdir() if path.suffix != ".app"),
        *sorted((current / "Libraries").glob("*.dylib")),
        versions.parent,
        app,
    ]
    for inside in insides:
        subprocess.run(
            ["codesign", "--force", "--timestamp", "--options", "runtime", "--sign", identity, str(inside)],
            check=True,
            stdout=sys.stderr,
        )


def notarize_mac(app: Path, output: Path) -> None:
    profile = os.environ.get("KODOSI_NOTARY_PROFILE")
    if not profile:
        return
    archive = output / "notarize.zip"
    subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app), str(archive)], check=True)
    subprocess.run(
        ["xcrun", "notarytool", "submit", str(archive), "--keychain-profile", profile, "--wait"],
        check=True,
        stdout=sys.stderr,
    )
    archive.unlink()
    subprocess.run(["xcrun", "stapler", "staple", str(app)], check=True, stdout=sys.stderr)


def background_app(app: Path) -> None:
    """No Dock icon and no place in the app switcher, and links from other apps never come."""
    path = app / "Contents/Info.plist"
    with path.open("rb") as file:
        info = plistlib.load(file)
    info["LSUIElement"] = True
    info["CFBundleIdentifier"] = APP_ID
    for name in NOT_FOR_THE_MAC_APP:
        info.pop(name, None)
    with path.open("wb") as file:
        plistlib.dump(info, file)


def mac_parts(out: Path, stage: Path) -> Path:
    app = stage / f"{PRODUCT}.app"
    shutil.copytree(out / f"{PRODUCT}.app", app, symlinks=True)
    background_app(app)
    sign_mac(app)
    notarize_mac(app, stage)
    return app


def plain_parts(out: Path, stage: Path, parts: tuple[str, ...], program: str) -> Path:
    folder = stage / "kodosi-browser"
    folder.mkdir()
    for part in parts:
        source_path = out / part
        if not source_path.exists():
            raise SystemExit(f"The build has no {part}")
        if source_path.is_dir():
            shutil.copytree(source_path, folder / part)
        else:
            shutil.copy2(source_path, folder / part)
    return folder / program


def package(out: Path, output: Path, version: str) -> Path:
    name = platform_name()
    stage = output / f"stage-{name}"
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    if name.startswith("mac"):
        program = mac_parts(out, stage).name
    elif name.startswith("windows"):
        program = plain_parts(out, stage, WINDOWS_PARTS, "chrome.exe").relative_to(stage).as_posix()
    else:
        program = plain_parts(out, stage, LINUX_PARTS, "chrome").relative_to(stage).as_posix()
    archive = output / f"kodosi-browser-{version}-{name}.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for path in sorted(stage.iterdir()):
            tar.add(path, arcname=path.name)
    shutil.rmtree(stage)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    entry = {"version": version, "platform": name, "file": archive.name, "program": program, "sha256": digest}
    (output / f"{archive.name}.json").write_text(json.dumps(entry, indent=2) + "\n", "utf-8")
    return archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    version = pin()
    src = source(version)
    brand(src)
    out = build(src)
    arguments.output_dir.mkdir(parents=True, exist_ok=True)
    print(package(out, arguments.output_dir, version))


if __name__ == "__main__":
    main()
