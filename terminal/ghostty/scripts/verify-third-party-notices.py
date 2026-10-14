#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tarfile
from datetime import date
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
MANIFEST_PATH = ROOT / "ThirdPartyNotices/inventory.json"
NOTICE_ROOT = ROOT / "ThirdPartyNotices/licenses"
SUMMARY_PATH = ROOT / "THIRD_PARTY_NOTICES.md"
NATIVE_BUILD = "NativeBuild.env"
MAC_SLICE = "macos-arm64"
LIBINTL_SOURCE = PurePosixPath("ThirdPartyNotices/corresponding-source/libintl-0.24")
LIBINTL_ARCHIVE = f"{LIBINTL_SOURCE}.tar"
LIBINTL_MANIFEST = f"{LIBINTL_SOURCE}/MANIFEST.json"
Z2D_RECORD_KEYS = {
    "version",
    "commit",
    "zigIdentity",
    "primaryUrl",
    "corroboratingUrl",
    "method",
    "acquiredOn",
    "archivePath",
    "byteSize",
    "sha256",
}


def fail(message: str) -> None:
    raise SystemExit(f"[!] third-party compliance: {message}")


def run(*args: str) -> str:
    env = os.environ.copy()
    env["LC_ALL"] = "C"
    try:
        return subprocess.check_output(args, cwd=ROOT, env=env, text=True)
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"command failed: {' '.join(args)} ({error})")


def load_manifest() -> dict:
    try:
        manifest = json.loads(MANIFEST_PATH.read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {MANIFEST_PATH.relative_to(ROOT)}: {error}")
    if manifest.get("schemaVersion") != 2:
        fail("unsupported inventory schema")
    return manifest


def load_json(relative: str) -> dict:
    try:
        value = json.loads(safe_file(relative).read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {relative}: {error}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain a JSON object")
    return value


def apple_slices() -> dict[str, dict]:
    options = load_metadata(NATIVE_BUILD)
    names = options.get("GHOSTTY_BUILD_APPLE_SLICES", "").split(",")
    if MAC_SLICE not in names or "" in names or len(names) != len(set(names)):
        fail(f"{NATIVE_BUILD} must list each Apple slice once, with {MAC_SLICE}")
    slices: dict[str, dict] = {}
    for name in names:
        platform, _, rest = name.partition("-")
        architecture, _, variant = rest.partition("-")
        i18n = options.get(f"GHOSTTY_BUILD_{platform.upper()}_I18N")
        if architecture != "arm64" or variant not in {"", "simulator"} or i18n not in {"true", "false"}:
            fail(f"{NATIVE_BUILD} has no complete build values for the Apple slice {name}")
        slices[name] = {
            "platform": platform,
            "variant": variant or None,
            "i18n": i18n == "true",
        }
    return slices


def safe_file(relative: str) -> Path:
    pure = PurePosixPath(relative)
    if pure.is_absolute() or ".." in pure.parts:
        fail(f"unsafe tracked path: {relative}")
    path = ROOT.joinpath(*pure.parts)
    try:
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(ROOT.resolve()):
            fail(f"missing or unsafe tracked file: {relative}")
    except OSError as error:
        fail(f"cannot inspect tracked file {relative}: {error}")
    return path


def require_git_owned(relative: str) -> None:
    tracked = run("git", "ls-files", "--error-unmatch", "--", relative).strip()
    if tracked != relative:
        fail(f"compliance file is not Git-owned: {relative}")


def verify_principal_licenses() -> None:
    package_license = safe_file("LICENSE").read_text()
    ghostty_license = safe_file("LICENSE-GHOSTTY").read_text()
    if "Copyright (c) 2026 @Lakr233" not in package_license:
        fail("LICENSE must preserve the imported package lineage notice")
    if "Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors" not in ghostty_license:
        fail("LICENSE-GHOSTTY must preserve Ghostty's notice")


def verify_compliance_artifacts(manifest: dict, working_tree: bool) -> None:
    artifacts = manifest.get("complianceArtifacts")
    required = {
        LIBINTL_ARCHIVE,
        LIBINTL_MANIFEST,
        z2d_acquisition(manifest)["archivePath"],
        "scripts/relink-libintl.sh",
    }
    if not isinstance(artifacts, dict) or set(artifacts) != required:
        fail("compliance artifact set differs from the exact contract")
    for relative, record in artifacts.items():
        if set(record) != {"sha256"}:
            fail(f"invalid compliance artifact record: {relative}")
        path = safe_file(relative)
        if not working_tree:
            require_git_owned(relative)
        if hashlib.sha256(path.read_bytes()).hexdigest() != record["sha256"]:
            fail(f"compliance artifact changed without inventory update: {relative}")
    if not os.access(safe_file("scripts/relink-libintl.sh"), os.X_OK):
        fail("libintl relink helper must be executable")

    source_manifest = load_json(LIBINTL_MANIFEST)
    sources = source_manifest.get("compiledSources")
    if not isinstance(sources, list) or not sources or len(sources) != len(set(sources)):
        fail("libintl corresponding-source manifest must name unique compiled sources")
    if any(not isinstance(source, str) or not source.endswith(".c") for source in sources):
        fail("libintl compiled-source manifest contains a non-C entry")
    source_root = ROOT.joinpath(*LIBINTL_SOURCE.parts)
    file_records = source_manifest.get("files")
    if not isinstance(file_records, list) or not file_records:
        fail("libintl corresponding-source file manifest is empty")
    expected_tar_members: set[str] = set()
    for record in file_records:
        if set(record) != {"path", "sha256"}:
            fail("invalid libintl corresponding-source file record")
        relative = PurePosixPath(record["path"])
        if relative.is_absolute() or ".." in relative.parts:
            fail(f"unsafe libintl source path: {relative}")
        path = source_root.joinpath(*relative.parts)
        if not path.is_file() or path.is_symlink():
            fail(f"missing libintl corresponding source: {relative}")
        if hashlib.sha256(path.read_bytes()).hexdigest() != record["sha256"]:
            fail(f"libintl corresponding source changed: {relative}")
        expected_tar_members.add(f"{LIBINTL_SOURCE.name}/{relative.as_posix()}")
    expected_tar_members.add(f"{LIBINTL_SOURCE.name}/MANIFEST.json")
    with tarfile.open(safe_file(LIBINTL_ARCHIVE)) as archive:
        actual_tar_members = {member.name for member in archive.getmembers() if member.isfile()}
    if actual_tar_members != expected_tar_members:
        fail("libintl corresponding-source tar does not match its tracked manifest tree")

    verify_z2d_source_acquisition(manifest)


def z2d_acquisition(manifest: dict) -> dict:
    acquisitions = manifest.get("sourceAcquisitions")
    if not isinstance(acquisitions, dict) or set(acquisitions) != {"z2d"}:
        fail("source acquisition set differs from the exact contract")
    record = acquisitions["z2d"]
    if not isinstance(record, dict) or set(record) != Z2D_RECORD_KEYS:
        fail("z2d acquisition record has unexpected fields")
    return record


def iso_date(value: object) -> bool:
    if not isinstance(value, str):
        return False
    try:
        return date.fromisoformat(value).isoformat() == value
    except ValueError:
        return False


def verify_z2d_source_acquisition(manifest: dict) -> None:
    record = z2d_acquisition(manifest)
    commit = record["commit"]
    version = record["version"]
    if (
        not isinstance(commit, str)
        or not re.fullmatch(r"[0-9a-f]{40}", commit)
        or not isinstance(version, str)
        or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version)
        or record["primaryUrl"] != f"https://deps.files.ghostty.org/z2d-{commit}.tar.gz"
        or record["corroboratingUrl"] != f"https://codeload.github.com/vancluever/z2d/tar.gz/{commit}"
        or record["archivePath"] != f"ThirdPartyNotices/corresponding-source/z2d-{commit}.tar.gz"
        or not isinstance(record["zigIdentity"], str)
        or not record["zigIdentity"].startswith(f"z2d-{version}-")
        or not isinstance(record["method"], str)
        or not record["method"]
        or not iso_date(record["acquiredOn"])
        or type(record["byteSize"]) is not int
        or not isinstance(record["sha256"], str)
        or not re.fullmatch(r"[0-9a-f]{64}", record["sha256"])
    ):
        fail("z2d acquisition metadata differs from its commit, version, date or digest format")

    z2d = next(
        (component for component in manifest.get("components", []) if component.get("id") == "z2d"),
        None,
    )
    expected_source_paths = [
        record["archivePath"],
        f"pinned-source:{record['primaryUrl']}",
        f"pinned-source:{record['corroboratingUrl']}",
    ]
    if (
        z2d is None
        or z2d.get("identity") != record["zigIdentity"]
        or z2d.get("version") != f"{version} / {commit}"
        or z2d.get("sourcePaths") != expected_source_paths
    ):
        fail("z2d component ownership differs from the sealed source acquisition")

    archive_path = safe_file(record["archivePath"])
    archive_bytes = archive_path.read_bytes()
    if len(archive_bytes) != record["byteSize"]:
        fail("z2d source archive size differs")
    if hashlib.sha256(archive_bytes).hexdigest() != record["sha256"]:
        fail("z2d source archive digest differs")

    top_level = f"z2d-{commit}"
    required_members = {
        f"{top_level}/LICENSE",
        f"{top_level}/COPYING",
        f"{top_level}/build.zig.zon",
    }
    regular_members: dict[str, bytes] = {}
    try:
        with tarfile.open(archive_path, mode="r:gz") as archive:
            for member in archive.getmembers():
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts:
                    fail(f"unsafe z2d archive member: {member.name}")
                if not path.parts or path.parts[0] != top_level:
                    fail(f"unexpected z2d archive root: {member.name}")
                if member.issym() or member.islnk():
                    fail(f"z2d archive contains a link: {member.name}")
                if member.isfile():
                    stream = archive.extractfile(member)
                    if stream is None:
                        fail(f"cannot read z2d archive member: {member.name}")
                    regular_members[member.name] = stream.read()
                elif not member.isdir():
                    fail(f"z2d archive contains an unsupported member: {member.name}")
    except (OSError, tarfile.TarError) as error:
        fail(f"cannot inspect z2d source archive: {error}")
    if not required_members.issubset(regular_members):
        fail("z2d source archive lacks required identity or license files")

    build_zon = regular_members[f"{top_level}/build.zig.zon"].decode("utf-8")
    if not re.search(rf'\.version\s*=\s*"{re.escape(version)}"', build_zon):
        fail("z2d source archive declares a different version")
    for member, notice in (
        (f"{top_level}/LICENSE", "ThirdPartyNotices/licenses/z2d-NOTICE.txt"),
        (f"{top_level}/COPYING", "ThirdPartyNotices/licenses/z2d-MPL-2.0.txt"),
    ):
        if hashlib.sha256(regular_members[member]).hexdigest() != hashlib.sha256(
            safe_file(notice).read_bytes()
        ).hexdigest():
            fail(f"z2d archived license differs from tracked notice: {notice}")


def verify_notices(manifest: dict) -> None:
    if not SUMMARY_PATH.is_file():
        fail("THIRD_PARTY_NOTICES.md is missing")
    declared = manifest.get("notices")
    if not isinstance(declared, dict) or not declared:
        fail("notice inventory is empty")
    actual = {
        path.name
        for path in NOTICE_ROOT.iterdir()
        if path.is_file() and not path.is_symlink()
    }
    if actual != set(declared):
        fail(
            "notice file set differs from inventory "
            f"(missing={sorted(set(declared) - actual)}, extra={sorted(actual - set(declared))})"
        )
    for notice_id, record in declared.items():
        if set(record) != {"path", "sha256"}:
            fail(f"invalid notice record: {notice_id}")
        expected_path = f"ThirdPartyNotices/licenses/{notice_id}"
        if record["path"] != expected_path:
            fail(f"notice path mismatch for {notice_id}")
        digest = hashlib.sha256(safe_file(record["path"]).read_bytes()).hexdigest()
        if digest != record["sha256"]:
            fail(f"notice text changed without inventory update: {notice_id}")


def verify_components(manifest: dict) -> set[str]:
    components = manifest.get("components")
    if not isinstance(components, list) or not components:
        fail("component inventory is empty")
    ids: set[str] = set()
    referenced_notices: set[str] = set()
    for component in components:
        required = {"id", "name", "version", "identity", "spdx", "notices", "sourcePaths", "scope"}
        if set(component) != required:
            fail(f"invalid component record: {component.get('id', '<unknown>')}")
        component_id = component["id"]
        if component["scope"] not in {"archive", "package"}:
            fail(f"invalid component scope: {component_id}")
        if component_id in ids:
            fail(f"duplicate component: {component_id}")
        ids.add(component_id)
        if not component["identity"] or not component["sourcePaths"]:
            fail(f"component lacks pinned source identity: {component_id}")
        for source_path in component["sourcePaths"]:
            if source_path.startswith(("pinned-source:", "provenance-only:")):
                _, _, identity = source_path.partition(":")
                if not identity:
                    fail(f"component has empty external source identity: {component_id}")
            else:
                safe_file(source_path)
        for notice in component["notices"]:
            if notice == "LICENSE-GHOSTTY":
                safe_file(notice)
            elif notice not in manifest["notices"]:
                fail(f"component {component_id} references unknown notice {notice}")
            else:
                referenced_notices.add(notice)
    unreferenced = set(manifest["notices"]) - referenced_notices
    if unreferenced:
        fail(f"unreferenced notice files: {sorted(unreferenced)}")
    return ids


def load_metadata(relative: str = "Vendor/libghostty.version") -> dict[str, str]:
    metadata: dict[str, str] = {}
    for line in safe_file(relative).read_text().splitlines():
        if not line or "=" not in line:
            fail(f"malformed {relative}")
        key, value = line.split("=", 1)
        if key in metadata:
            fail(f"duplicate provenance key in {relative}: {key}")
        metadata[key] = value
    return metadata


def slice_archive(slice_name: str) -> str:
    return f"Vendor/GhosttyKit.xcframework/{slice_name}/libghostty.a"


def libintl_object_members() -> list[str]:
    return [
        PurePosixPath(source).with_suffix(".o").name
        for source in load_json(LIBINTL_MANIFEST)["compiledSources"]
    ]


def verify_archive(manifest: dict, component_ids: set[str], slices: dict[str, dict]) -> Path:
    archive_paths = manifest.get("archives")
    if archive_paths != [
        slice_archive(MAC_SLICE),
        f"Vendor/GhosttyVt/{MAC_SLICE}/lib/libghostty-vt.a",
    ]:
        fail("shipping archive path set changed")
    archives = [safe_file(path) for path in archive_paths]
    all_vendored_archives = sorted(
        path
        for path in (ROOT / "Vendor").rglob("*.a")
        if "linux-x86_64" not in path.parts
    )
    declared_archives = {path.resolve() for path in archives} | {
        safe_file(slice_archive(slice_name)).resolve() for slice_name in slices
    }
    if {path.resolve() for path in all_vendored_archives} != declared_archives:
        fail("vendored static archive set differs from the declared Apple slice images")
    archs = run("lipo", "-archs", str(archives[0])).split()
    if archs != ["arm64"]:
        fail(f"combined archive architectures differ: {archs}")

    actual_members = run("ar", "-t", str(archives[0])).splitlines()
    member_records = manifest.get("archiveMembers")
    expected_members = [record.get("name") for record in member_records]
    if actual_members != expected_members:
        missing = sorted(set(expected_members) - set(actual_members))
        extra = sorted(set(actual_members) - set(expected_members))
        fail(f"archive member inventory changed (missing={missing}, extra={extra})")
    if len(actual_members) != len(set(actual_members)):
        fail("archive contains duplicate member names")
    if not actual_members or actual_members[0] != "__.SYMDEF SORTED":
        fail("archive index member changed")
    owned_components: set[str] = set()
    for record in member_records:
        name = record["name"]
        owners = record.get("owners")
        if name == "__.SYMDEF SORTED":
            if owners:
                fail("archive index must not have an owner")
            continue
        if not owners:
            fail(f"archive member has no component owner: {name}")
        unknown = set(owners) - component_ids
        if unknown:
            fail(f"archive member {name} has unknown owners: {sorted(unknown)}")
        owned_components.update(owners)
    archive_component_ids = {
        component["id"] for component in manifest["components"] if component["scope"] == "archive"
    }
    if owned_components != archive_component_ids:
        fail(
            "archive components without ownership: "
            f"{sorted(archive_component_ids - owned_components)}"
        )
    return archives[0]


def verify_evidence(
    manifest: dict,
    archive: Path,
    absent: frozenset[str] = frozenset(),
    label: str = MAC_SLICE,
) -> None:
    symbols = run("nm", "-pa", str(archive))
    printable_strings = run("strings", str(archive))
    members = set(run("ar", "-t", str(archive)).splitlines())
    covered: set[str] = set()
    component_ids = {
        component["id"] for component in manifest["components"] if component["scope"] == "archive"
    } - absent
    for record in manifest["archiveMembers"]:
        if record["name"] not in {"__.SYMDEF SORTED", "libghostty_zcu.o"}:
            covered.update(set(record["owners"]) - absent)
    for evidence in manifest.get("archiveEvidence", []):
        if set(evidence) != {"component", "kind", "pattern"}:
            fail("invalid archive evidence record")
        component = evidence["component"]
        if component not in component_ids | absent:
            fail(f"evidence references unknown component: {component}")
        kind, pattern = evidence["kind"], evidence["pattern"]
        if kind == "member":
            found = pattern in members
        elif kind == "symbol":
            found = pattern in symbols
        elif kind == "string":
            found = pattern in printable_strings
        else:
            fail(f"unknown archive evidence kind: {kind}")
        if component in absent:
            if found:
                fail(f"{label} archive contains {component}, which its notices declare absent: {kind} {pattern}")
            continue
        if not found:
            fail(f"{label} archive evidence missing for {component}: {kind} {pattern}")
        covered.add(component)
    if covered != component_ids:
        fail(f"{label} components lack deterministic archive evidence: {sorted(component_ids - covered)}")

    libintl_members = [
        record["name"]
        for record in manifest["archiveMembers"]
        if "gettext-libintl" in record.get("owners", [])
    ]
    if (
        sorted(libintl_members) != sorted(libintl_object_members())
        or len(libintl_members) != len(set(libintl_members))
    ):
        fail(f"libintl archive members in {label} differ from the compiled sources in {LIBINTL_MANIFEST}")
    if "gettext-libintl" in absent:
        if members & set(libintl_members) or "_libintl_version" in symbols:
            fail(f"{label} archive contains libintl, which its notices declare absent")
    elif "_libintl_version" not in symbols:
        fail(f"libintl version symbol is missing from the {label} archive")


def verify_apple_slices(manifest: dict, slices: dict[str, dict]) -> None:
    ios_slices = [slice_name for slice_name in slices if slice_name != MAC_SLICE]
    records = manifest.get("appleSlices")
    if not isinstance(records, dict) or set(records) != set(ios_slices):
        fail(f"Apple slice inventory differs from the slices in {NATIVE_BUILD}")
    if not slices[MAC_SLICE]["i18n"]:
        fail(f"{MAC_SLICE} builds without i18n, but its notices include libintl")
    archive_component_ids = {
        component["id"] for component in manifest["components"] if component["scope"] == "archive"
    }
    for slice_name in ios_slices:
        record = records[slice_name]
        if not isinstance(record, dict) or set(record) != {"archive", "absentComponents"}:
            fail(f"invalid Apple slice record: {slice_name}")
        if record["archive"] != slice_archive(slice_name):
            fail(f"{slice_name} archive path changed")
        absent = record["absentComponents"]
        if (
            not isinstance(absent, list)
            or absent != sorted(set(absent))
            or not set(absent) <= archive_component_ids
            or "ghostty" in absent
        ):
            fail(f"{slice_name} absent component list is invalid")
        if slices[slice_name]["i18n"] == ("gettext-libintl" in absent):
            fail(f"{slice_name} i18n build option differs from its libintl notice")
        expected_members = [
            member["name"]
            for member in manifest["archiveMembers"]
            if not member["owners"] or not set(member["owners"]) <= set(absent)
        ]
        archive = safe_file(record["archive"])
        if run("lipo", "-archs", str(archive)).split() != ["arm64"]:
            fail(f"{slice_name} archive architectures differ")
        if run("ar", "-t", str(archive)).splitlines() != expected_members:
            fail(f"{slice_name} archive member inventory differs from the declared members")
        verify_evidence(manifest, archive, frozenset(absent), slice_name)


def verify_pins(manifest: dict, slices: dict[str, dict]) -> None:
    metadata = load_metadata()
    platform_ref = safe_file("MacOSGhostty.ref").read_text()
    if platform_ref != f"{manifest['ghosttyCommit']}\n":
        fail("MacOSGhostty.ref differs from the notice inventory")
    if metadata.get("ghostty_commit") != manifest["ghosttyCommit"]:
        fail("artifact Ghostty commit differs from inventory")
    ghostty = next(component for component in manifest["components"] if component["id"] == "ghostty")
    expected_ghostty_identity = (
        f"ghostty:{manifest['ghosttyCommit']};"
        f"patch_set:{metadata.get('patches_sha256', '')};"
        f"patch_application:{metadata.get('patch_application_sha256', '')}"
    )
    if ghostty["identity"] != expected_ghostty_identity:
        fail("Ghostty composite source and patch identity differs from artifact provenance")
    if metadata.get("zig_store_sha256") != manifest.get("zigStoreSha256"):
        fail("artifact Zig store identity differs from inventory")

    info = run(
        "plutil",
        "-convert",
        "json",
        "-o",
        "-",
        "Vendor/GhosttyKit.xcframework/Info.plist",
    )
    available = json.loads(info).get("AvailableLibraries", [])
    libraries = {
        library.get("LibraryIdentifier"): (
            library.get("SupportedPlatform"),
            library.get("SupportedPlatformVariant"),
            library.get("SupportedArchitectures"),
        )
        for library in available
    }
    expected_libraries = {
        name: (values["platform"], values["variant"], ["arm64"])
        for name, values in slices.items()
    }
    if len(available) != len(expected_libraries) or libraries != expected_libraries:
        fail(f"XCFramework slices differ from the arm64 Apple slices in {NATIVE_BUILD}")

    package_manifest = safe_file("Package.swift").read_text()
    if re.search(r"\.package\s*\(", package_manifest):
        fail("Package.swift contains a dependency missing from the notice inventory")

    zig = next(component for component in manifest["components"] if component["id"] == "zig-runtime")
    expected_zig_hash = zig["identity"].removeprefix("sha256:")
    toolchain = safe_file("Toolchain.env").read_text()
    match = re.search(r"^ZIG_AARCH64_MACOS_SHA256=(\S+)$", toolchain, re.MULTILINE)
    if not match or match.group(1) != expected_zig_hash:
        fail("Zig toolchain identity differs from inventory")


def main() -> None:
    parser = argparse.ArgumentParser(description="Verify the Ghostty notices of the Apple artifacts.")
    parser.add_argument(
        "--working-tree",
        action="store_true",
        help="accept compliance files that Git does not track yet. Use this for a build that you do not publish.",
    )
    arguments = parser.parse_args()
    manifest = load_manifest()
    slices = apple_slices()
    verify_principal_licenses()
    verify_compliance_artifacts(manifest, arguments.working_tree)
    verify_notices(manifest)
    component_ids = verify_components(manifest)
    archive = verify_archive(manifest, component_ids, slices)
    verify_evidence(manifest, archive)
    verify_apple_slices(manifest, slices)
    verify_pins(manifest, slices)
    print(f"[*] verified notices for {len(component_ids)} bundled third-party/component identities")


if __name__ == "__main__":
    main()
