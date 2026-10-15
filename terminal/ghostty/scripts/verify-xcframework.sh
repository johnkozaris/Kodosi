#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."
XCFRAMEWORK_PATH=${1:-}

if [ -z "$XCFRAMEWORK_PATH" ]; then
    echo "Usage: $0 <xcframework>"
    exit 1
fi

if [ ! -d "$XCFRAMEWORK_PATH" ]; then
    echo "[!] xcframework not found: $XCFRAMEWORK_PATH"
    exit 1
fi

APPLE_SLICES=$(./scripts/native-build-options.sh slices)
SLICES=()
while IFS= read -r slice; do
    SLICES+=("$slice")
done <<<"$APPLE_SLICES"

python3 - "$XCFRAMEWORK_PATH" "${SLICES[@]}" <<'PY'
import os
import plistlib
import sys

xcframework = sys.argv[1]
expected_slices = {}
for identifier in sys.argv[2:]:
    platform, _, rest = identifier.partition("-")
    _, _, variant = rest.partition("-")
    expected_slices[identifier] = (platform, variant or None)
info_path = os.path.join(xcframework, "Info.plist")

if not os.path.isfile(info_path):
    raise SystemExit(f"[!] missing xcframework Info.plist: {info_path}")

for root, dirs, _ in os.walk(xcframework):
    for directory in dirs:
        if directory.endswith(".framework"):
            raise SystemExit(f"[!] framework wrapper is not allowed in static-library xcframework: {os.path.join(root, directory)}")

with open(info_path, "rb") as handle:
    info = plistlib.load(handle)

libraries = info.get("AvailableLibraries")
if not isinstance(libraries, list) or len(libraries) != len(expected_slices):
    raise SystemExit("[!] Expected exactly one arm64 library for each Apple slice in NativeBuild.env")

identifiers = [library.get("LibraryIdentifier") for library in libraries]
if sorted(map(str, identifiers)) != sorted(expected_slices):
    raise SystemExit(f"[!] unexpected xcframework slices: {identifiers}")

for library in libraries:
    identifier = library.get("LibraryIdentifier")
    library_path = library.get("LibraryPath")
    binary_path = library.get("BinaryPath")
    headers_path = library.get("HeadersPath")
    platform = library.get("SupportedPlatform")
    variant = library.get("SupportedPlatformVariant")

    if (platform, variant) != expected_slices[identifier] or library.get("SupportedArchitectures") != ["arm64"]:
        raise SystemExit(f"[!] unsupported platform in {identifier}: {platform} {variant}")
    if library_path != "libghostty.a" or binary_path != "libghostty.a":
        raise SystemExit(f"[!] {identifier} must reference libghostty.a, got LibraryPath={library_path!r} BinaryPath={binary_path!r}")
    if headers_path != "Headers":
        raise SystemExit(f"[!] {identifier} must use HeadersPath=Headers, got {headers_path!r}")

    variant_dir = os.path.join(xcframework, identifier)
    archive_path = os.path.join(variant_dir, library_path)
    headers_dir = os.path.join(variant_dir, headers_path)
    header_path = os.path.join(headers_dir, "ghostty.h")
    modulemap_path = os.path.join(headers_dir, "module.modulemap")

    for required_path in (archive_path, header_path, modulemap_path):
        if not os.path.exists(required_path):
            raise SystemExit(f"[!] {identifier} missing required file: {required_path}")

    with open(modulemap_path, "r", encoding="utf-8") as handle:
        modulemap = handle.read()
    if "framework module libghostty" in modulemap:
        raise SystemExit(f"[!] {identifier} module map must not declare a framework module")
    if "module libghostty" not in modulemap:
        raise SystemExit(f"[!] {identifier} module map does not declare module libghostty")

print(f"[*] verified static-library xcframework layout: {xcframework}")
PY
