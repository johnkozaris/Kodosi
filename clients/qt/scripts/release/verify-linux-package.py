#!/usr/bin/env python3
"""Check a Debian, portable or Arch package of Kodosi."""

from __future__ import annotations

import argparse
import io
import json
import os
import posixpath
import re
import subprocess
import sys
import tarfile
import tempfile
import unicodedata
import zlib
from pathlib import Path, PurePosixPath

from package_evidence import verify_rust_licenses, verify_source_identity
from release_source import product_version, source_identity

ROOT = Path(__file__).resolve().parents[2]
GHOSTTY = ROOT.parents[1] / "terminal/ghostty"
KINDS = {".deb": "deb", ".tar.gz": "tgz", ".pkg.tar.zst": "arch"}
DOC = "usr/share/doc/kodosi"
PRIVATE = "usr/lib/kodosi"
WEB_ENGINE_PROCESS = f"{PRIVATE}/libexec/QtWebEngineProcess"
DESKTOP = "usr/share/applications/com.kodosi.Kodosi.desktop"
METAINFO = "usr/share/metainfo/com.kodosi.Kodosi.metainfo.xml"
PLUGINS = (
    "plugins/platforms/libqxcb.so",
    "plugins/platforms/libqwayland.so",
    "plugins/platforms/libqoffscreen.so",
    "plugins/platformthemes/libqgtk3.so",
    "plugins/platformthemes/libqxdgdesktopportal.so",
    "qml/QtQuick/Controls/libqtquickcontrols2plugin.so",
)
# The app loads or starts these at run time, so no library link shows them.
DEBIAN_DEPENDENCIES = ("libopengl0", "desktop-file-utils", "xdg-desktop-portal", "xdg-utils")


class PackageError(Exception):
    pass


def fail(message: str) -> None:
    raise PackageError(message)


def check_text(value: str, description: str) -> None:
    if "\\" in value or any(
        unicodedata.category(character) in {"Cc", "Cs"} for character in value
    ):
        fail(f"unsafe {description}: {value!r}")


def archive_path(name: str, *, allow_root: bool = False) -> str:
    check_text(name, "archive path")
    candidate = name[2:] if name.startswith("./") else name
    if candidate in {"", "."}:
        if allow_root:
            return "."
        fail(f"unsafe archive path: {name!r}")
    if posixpath.isabs(candidate) or any(
        part in {".", ".."} for part in candidate.split("/")
    ):
        fail(f"unsafe archive path: {name!r}")
    normalized = posixpath.normpath(candidate)
    if normalized in {"", ".", ".."}:
        fail(f"unsafe archive path: {name!r}")
    return normalized


def member_path(member: tarfile.TarInfo) -> str:
    return archive_path(member.name, allow_root=member.isdir())


def symlink_target(member_path: str, target: str) -> str:
    check_text(target, "symlink target")
    if not target or posixpath.isabs(target):
        fail(f"unsafe symlink target: {target!r}")
    parent = posixpath.dirname(member_path)
    normalized = posixpath.normpath(posixpath.join(parent, target))
    path = PurePosixPath(normalized)
    if (
        normalized in {"", ".."}
        or path.is_absolute()
        or any(part == ".." for part in path.parts)
    ):
        fail(f"unsafe symlink target: {target!r}")
    return path.as_posix()


def hardlink_target(target: str) -> str:
    check_text(target, "hardlink target")
    return archive_path(target)


def validate_member(member: tarfile.TarInfo, epoch: int) -> str:
    name = member.name
    normalized = member_path(member)
    if not (
        member.isfile()
        or member.isdir()
        or member.issym()
        or member.islnk()
    ):
        fail(f"unsupported archive member type: {name}")
    if member.mtime != epoch:
        fail(f"archive mtime differs from SOURCE_DATE_EPOCH: {name}")
    if member.uid != 0 or member.gid != 0:
        fail(f"archive ownership is not root:root: {name}")
    if member.uname not in {"", "root"} or member.gname not in {"", "root"}:
        fail(f"archive owner names are not normalized: {name}")
    if any(key in member.pax_headers for key in ("atime", "ctime")):
        fail(f"archive contains host filesystem timestamps: {name}")
    mode = member.mode & 0o7777
    if member.isdir() and mode != 0o755:
        fail(f"archive directory mode is not 0755: {name}")
    if (member.isfile() or member.islnk()) and mode not in {0o644, 0o755}:
        fail(f"archive file mode is not normalized: {name}")
    if member.issym() and mode != 0o777:
        fail(f"archive symlink mode is not 0777: {name}")
    return normalized


def resolve_link_path(
    path: str,
    links: dict[str, str],
) -> str:
    current = path
    visited: set[str] = set()
    while True:
        parts = PurePosixPath(current).parts
        matched = False
        for length in range(1, len(parts) + 1):
            prefix = PurePosixPath(*parts[:length]).as_posix()
            target = links.get(prefix)
            if target is None:
                continue
            if prefix in visited:
                fail(f"archive link chain is cyclic: {path}")
            visited.add(prefix)
            remainder = parts[length:]
            current = (
                PurePosixPath(target, *remainder).as_posix()
                if remainder
                else target
            )
            if current == ".." or current.startswith("../"):
                fail(f"archive link chain escapes root: {path}")
            matched = True
            break
        if not matched:
            return current


def validate_tar(
    archive: tarfile.TarFile,
    epoch: int,
    *,
    require_pkginfo: bool,
    expected_orders: list[list[str]] | None = None,
) -> None:
    names: list[str] = []
    members: dict[str, tarfile.TarInfo] = {}
    links: dict[str, str] = {}
    pkginfo: bytes | None = None
    for member in archive:
        normalized = validate_member(member, epoch)
        names.append(member.name)
        if normalized in members:
            fail(f"archive contains duplicate normalized path: {normalized}")
        members[normalized] = member
        if member.issym():
            links[normalized] = symlink_target(normalized, member.linkname)
        elif member.islnk():
            links[normalized] = hardlink_target(member.linkname)
        if normalized == ".PKGINFO":
            if not member.isfile():
                fail(".PKGINFO is not a regular file")
            source = archive.extractfile(member)
            if source is None:
                fail(".PKGINFO is not a regular file")
            pkginfo = source.read()
    for normalized, member in members.items():
        if normalized != ".":
            parts = PurePosixPath(normalized).parts
            for length in range(1, len(parts)):
                parent = PurePosixPath(*parts[:length]).as_posix()
                parent_member = members.get(parent)
                if parent_member is not None and not parent_member.isdir():
                    fail(
                        "archive member is nested below a non-directory: "
                        f"{normalized}"
                    )
        if member.issym():
            resolve_link_path(links[normalized], links)
        elif member.islnk():
            target = links[normalized]
            if target not in members:
                fail(f"archive hardlink target is missing: {member.linkname}")
            resolved = resolve_link_path(target, links)
            target_member = members.get(resolved)
            if target_member is None or not target_member.isfile():
                fail(
                    f"archive hardlink target is not a regular file: "
                    f"{member.linkname}"
                )
    if expected_orders is not None:
        if names not in expected_orders:
            fail(
                "archive metadata member order is not deterministic: "
                + ", ".join(names)
            )
    elif names != sorted(names):
        fail("archive paths are not sorted")
    if any(PurePosixPath(name).name == ".BUILDINFO" for name in members):
        fail("Arch package contains host-derived .BUILDINFO")
    if require_pkginfo:
        if pkginfo is None:
            fail("Arch package is missing .PKGINFO")
        text = pkginfo.decode("utf-8")
        if f"builddate = {epoch}\n" not in text:
            fail("Arch package builddate differs from SOURCE_DATE_EPOCH")
        for forbidden in ("buildhost", "/home/", "user = ", "buildenv = "):
            if forbidden in text:
                fail(f"Arch metadata contains host-derived field: {forbidden}")


def parse_ar(payload: bytes, epoch: int) -> dict[str, bytes]:
    if not payload.startswith(b"!<arch>\n"):
        fail("DEB is not an ar archive")
    offset = 8
    members: dict[str, bytes] = {}
    order: list[str] = []
    while offset < len(payload):
        header = payload[offset : offset + 60]
        if len(header) != 60 or header[58:60] != b"`\n":
            fail("DEB ar header is malformed")
        name = header[:16].decode("ascii").strip().rstrip("/")
        try:
            mtime = int(header[16:28].decode("ascii").strip())
            uid = int(header[28:34].decode("ascii").strip())
            gid = int(header[34:40].decode("ascii").strip())
            mode = int(header[40:48].decode("ascii").strip(), 8)
            size = int(header[48:58].decode("ascii").strip())
        except ValueError as error:
            fail(f"DEB ar metadata is malformed: {error}")
        if mtime != epoch or uid != 0 or gid != 0:
            fail(f"DEB ar metadata is not normalized: {name}")
        if mode & 0o777 != 0o644:
            fail(f"DEB ar member mode is not 0644: {name}")
        offset += 60
        body = payload[offset : offset + size]
        if len(body) != size:
            fail(f"DEB ar member is truncated: {name}")
        if name in members:
            fail(f"DEB ar member is duplicated: {name}")
        members[name] = body
        order.append(name)
        offset += size + (size % 2)
    if (
        len(order) != 3
        or order[0] != "debian-binary"
        or not order[1].startswith("control.tar.")
        or not order[2].startswith("data.tar.")
    ):
        fail("DEB ar member set or order is not deterministic")
    if members["debian-binary"] != b"2.0\n":
        fail("DEB format marker is invalid")
    return members


def validate_deb(path: Path, epoch: int) -> None:
    members = parse_ar(path.read_bytes(), epoch)
    control_name = next(name for name in members if name.startswith("control.tar."))
    data_name = next(name for name in members if name.startswith("data.tar."))
    for name in (control_name, data_name):
        with tarfile.open(fileobj=io.BytesIO(members[name]), mode="r:*") as archive:
            validate_tar(
                archive,
                epoch,
                require_pkginfo=False,
                expected_orders=(
                    [
                        ["./md5sums", "./control"],
                        ["./control", "./md5sums"],
                        [".", "./control"],
                        ["./md5sums", "./control", "./conffiles", "./postinst"],
                    ]
                    if name == control_name
                    else None
                ),
            )
    with tarfile.open(
        fileobj=io.BytesIO(members[control_name]),
        mode="r:*",
    ) as archive:
        control_member = archive.getmember("./control")
        source = archive.extractfile(control_member)
        if source is None:
            fail("DEB control metadata is missing")
        control = source.read().decode("utf-8")
    if re.search(r"^(Build-Date|Build-Host|Built-By):", control, re.MULTILINE):
        fail("DEB contains wall-clock or build-host metadata")


def validate_arch(path: Path, epoch: int) -> None:
    producer = subprocess.Popen(
        ["zstd", "--quiet", "--decompress", "--stdout", str(path)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert producer.stdout is not None
    try:
        with tarfile.open(fileobj=producer.stdout, mode="r|") as archive:
            validate_tar(archive, epoch, require_pkginfo=True)
    finally:
        producer.stdout.close()
    stderr = producer.communicate()[1]
    if producer.returncode != 0:
        fail(f"cannot decompress Arch package: {stderr.decode().strip()}")


def check_metadata(package: Path, kind: str, epoch: int) -> None:
    try:
        if kind == "deb":
            validate_deb(package, epoch)
        elif kind == "tgz":
            with tarfile.open(package, mode="r:gz") as archive:
                validate_tar(archive, epoch, require_pkginfo=False)
        else:
            validate_arch(package, epoch)
    except (EOFError, OSError, tarfile.TarError, zlib.error) as error:
        fail(f"the archive is damaged: {error}")


def run(*command: str) -> str:
    environment = {key: value for key, value in os.environ.items() if key != "LD_LIBRARY_PATH"}
    result = subprocess.run(
        command,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"{' '.join(command)} failed: {(result.stderr or result.stdout).strip()}")
    return result.stdout


def extract(package: Path, kind: str, stage: Path, version: str) -> Path:
    if kind == "deb":
        run("dpkg-deb", "-x", str(package), str(stage))
        return stage
    if kind == "arch":
        run("tar", "--zstd", "-xf", str(package), "-C", str(stage))
        return stage
    run("tar", "-xzf", str(package), "-C", str(stage))
    root = stage / f"kodosi-{version}-Linux"
    if not root.is_dir():
        fail(f"the archive has no folder kodosi-{version}-Linux")
    return root


def same(root: Path, name: str, source: Path) -> None:
    path = root / name
    if not path.is_file():
        fail(f"{name} is missing")
    if path.read_bytes() != source.read_bytes():
        fail(f"{name} differs from {source}")


def check_source_identity(root: Path, checkout: dict[str, object]) -> None:
    identity = root / DOC / "provenance/source-identity.json"
    if not identity.is_file():
        fail(f"{DOC}/provenance/source-identity.json is missing")
    try:
        verify_source_identity(identity.read_bytes(), checkout)
    except ValueError as error:
        fail(str(error))


def check_evidence(root: Path, build_dir: Path) -> None:
    rust = root / DOC / "rust"
    try:
        verify_rust_licenses(
            {
                path.relative_to(rust).as_posix(): path.read_bytes()
                for path in rust.rglob("*")
                if path.is_file()
            },
            build_dir / "generated/rust-dependency-licenses",
        )
    except ValueError as error:
        fail(str(error))
    same(root, f"{DOC}/provenance/dependencies.lock.json", ROOT / "dependencies.lock.json")

    for name in ("LICENSE", "LICENSE-GHOSTTY", "LinuxGhostty.ref", "THIRD_PARTY_NOTICES.md"):
        same(root, f"{DOC}/ghostty/{name}", GHOSTTY / name)
    inventory = GHOSTTY / "ThirdPartyNotices/linux-vt-inventory.json"
    same(root, f"{DOC}/ghostty/linux-vt-inventory.json", inventory)
    texts = {
        notice
        for component in json.loads(inventory.read_text(encoding="utf-8"))["components"]
        for notice in component["notices"]
    } - {"LICENSE-GHOSTTY"}
    folder = root / DOC / "ghostty/ThirdPartyNotices/licenses"
    packaged = {path.name for path in folder.iterdir()} if folder.is_dir() else set()
    if packaged != texts:
        fail(
            "the Ghostty license texts differ from linux-vt-inventory.json: "
            f"missing {sorted(texts - packaged)}, extra {sorted(packaged - texts)}"
        )
    for name in sorted(texts):
        same(
            root,
            f"{DOC}/ghostty/ThirdPartyNotices/licenses/{name}",
            GHOSTTY / "ThirdPartyNotices/licenses" / name,
        )

    for source in sorted((ROOT / "packaging/licenses/provider-icons").iterdir()):
        same(root, f"usr/share/licenses/kodosi-qt/provider-icons/{source.name}", source)
    same(
        root,
        f"{DOC}/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt",
        ROOT / "packaging/linux/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt",
    )
    run(
        sys.executable,
        str(ROOT / "scripts/release/verify-native-license-evidence.py"),
        "--installed-root",
        str(root),
    )


def check_debian_dependencies(package: Path) -> None:
    depends = run("dpkg-deb", "-f", str(package), "Depends").strip().split(", ")
    names = {item.split(" ", 1)[0] for item in depends}
    missing = [name for name in DEBIAN_DEPENDENCIES if name not in names]
    if missing:
        fail(f"the package does not depend on {', '.join(missing)}")


def check_files(root: Path, qt_version: str) -> None:
    for name in (
        "usr/bin/kodosi",
        "usr/bin/kodosi-qt",
        f"{PRIVATE}/bin/kodosi",
        f"{PRIVATE}/bin/kodosi-qt",
        f"{PRIVATE}/bin/agent-browser",
        WEB_ENGINE_PROCESS,
    ):
        path = root / name
        if not path.is_file() or not os.access(path, os.X_OK):
            fail(f"{name} is missing or is not executable")
    for name in (
        f"{PRIVATE}/bin/qt.conf",
        f"{DOC}/agent-browser/LICENSE",
        DESKTOP,
        METAINFO,
        "usr/share/icons/hicolor/256x256/apps/com.kodosi.Kodosi.png",
        *(f"{PRIVATE}/lib/libQt6{module}.so.{qt_version}" for module in ("Core", "DBus", "WebEngineCore", "Widgets")),
        *(f"{PRIVATE}/{plugin}" for plugin in PLUGINS),
        f"{PRIVATE}/resources/qtwebengine_resources.pak",
        f"{PRIVATE}/translations/qtwebengine_locales/en-US.pak",
    ):
        if not (root / name).is_file():
            fail(f"{name} is missing")
    settings = (root / PRIVATE / "bin/qt.conf").read_text(encoding="utf-8").splitlines()
    for line in ("Prefix = ../../..", f"LibraryExecutables = {PRIVATE.removeprefix('usr/')}/libexec", f"Data = {PRIVATE.removeprefix('usr/')}"):
        if line not in settings:
            fail(f"{PRIVATE}/bin/qt.conf has no line {line!r}")
    same(root, f"{PRIVATE}/libexec/qt.conf", root / PRIVATE / "bin/qt.conf")
    outside = sorted(path.name for path in (root / "usr").iterdir() if path.name not in {"bin", "lib", "share"})
    if outside:
        fail(f"the package puts files outside usr/bin, usr/lib and usr/share: {', '.join(outside)}")
    escaped = sorted(
        path.name
        for path in (root / "usr/lib").iterdir()
        if path.name.startswith(("libQt6", "libicu"))
    )
    if escaped:
        fail(f"the Qt or ICU runtime is outside {PRIVATE}: {', '.join(escaped)}")


def check_repository(package: Path, root: Path) -> None:
    same(root, "etc/apt/sources.list.d/kodosi.sources", ROOT / "packaging/linux/kodosi.sources")
    same(root, "etc/apt/keyrings/kodosi-archive-keyring.pgp", ROOT / "packaging/linux/kodosi-archive-keyring.pgp")
    same(root, "etc/apparmor.d/kodosi", ROOT / "packaging/linux/apparmor/kodosi")
    if run("dpkg-deb", "-I", str(package), "postinst") != (ROOT / "packaging/linux/postinst").read_text("utf-8"):
        fail("the postinst of the package differs from packaging/linux/postinst")
    if run("dpkg-deb", "-I", str(package), "conffiles") != (ROOT / "packaging/linux/conffiles").read_text("utf-8"):
        fail("the conffiles of the package differ from packaging/linux/conffiles")


def check_links(root: Path) -> None:
    cli = root / PRIVATE / "bin/kodosi"
    if not re.search(r"^\s*Class:\s+ELF64$", run("readelf", "-h", str(cli)), re.MULTILINE):
        fail(f"{PRIVATE}/bin/kodosi is not a 64-bit ELF program")
    private = os.path.normpath(root / PRIVATE) + os.sep
    for name in (f"{PRIVATE}/bin/kodosi", f"{PRIVATE}/bin/kodosi-qt", WEB_ENGINE_PROCESS, *(f"{PRIVATE}/{plugin}" for plugin in PLUGINS)):
        output = run("ldd", str(root / name))
        if "not found" in output:
            fail(f"{name} needs a library that the system and the package do not have:\n{output}")
        for line in output.splitlines():
            if "libQt6" not in line and "libicu" not in line:
                continue
            fields = line.split()
            target = fields[2] if len(fields) > 2 and fields[1] == "=>" else ""
            if not os.path.normpath(target).startswith(private):
                fail(f"{name} links {line.strip()}, which is outside {PRIVATE}")


def check_home(root: Path) -> None:
    home = str(Path.home()).encode("utf-8")
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        data = path.read_bytes()
        if (start := data.find(home)) >= 0:
            end = min(position for position in (data.find(b"\0", start), data.find(b"\n", start), len(data)) if position >= 0)
            named = data[start:end][:300].decode("utf-8", "replace")
            fail(f"{path.relative_to(root).as_posix()} names the home folder of the build: {named}")


def check_desktop(root: Path) -> None:
    lines = (root / DESKTOP).read_text(encoding="utf-8").splitlines()
    for line in ("Exec=kodosi-qt %u", "MimeType=x-scheme-handler/kodosi;"):
        if line not in lines:
            fail(f"{DESKTOP} has no line {line!r}")
    run("desktop-file-validate", str(root / DESKTOP))
    run("update-desktop-database", str(root / "usr/share/applications"))
    cache = (root / "usr/share/applications/mimeinfo.cache").read_text(encoding="utf-8")
    if not re.search(r"^x-scheme-handler/kodosi=.*com\.kodosi\.Kodosi\.desktop", cache, re.MULTILINE):
        fail("the desktop file does not open kodosi:// links")
    run("appstreamcli", "validate", "--no-net", str(root / METAINFO))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path, help="a .deb, .tar.gz or .pkg.tar.zst file")
    parser.add_argument(
        "--build-dir",
        required=True,
        type=Path,
        help="the release build folder that has the generated Rust license inventory",
    )
    arguments = parser.parse_args()
    package = arguments.package.resolve()
    kind = next((kind for suffix, kind in KINDS.items() if package.name.endswith(suffix)), None)
    try:
        if kind is None or not package.is_file():
            fail("the package must be an existing .deb, .tar.gz or .pkg.tar.zst file")
        try:
            checkout = source_identity(ROOT)
            version = product_version(ROOT)
            qt_version = json.loads((ROOT / "dependencies.lock.json").read_bytes())["frameworks"]["qt"]["version"]
        except (OSError, ValueError, KeyError) as error:
            fail(f"cannot read the checkout: {error}")
        (ROOT / "build").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="package-check-", dir=ROOT / "build") as stage:
            root = extract(package, kind, Path(stage), version)
            check_source_identity(root, checkout)
            check_metadata(package, kind, checkout["sourceDateEpoch"])
            check_evidence(root, arguments.build_dir.resolve())
            if kind == "deb":
                check_debian_dependencies(package)
                check_repository(package, root)
            check_files(root, qt_version)
            check_links(root)
            check_home(root)
            check_desktop(root)
    except PackageError as error:
        raise SystemExit(f"Linux package error: {package.name}: {error}") from None
    print(f"Linux package checked: {package.name}")


if __name__ == "__main__":
    main()
