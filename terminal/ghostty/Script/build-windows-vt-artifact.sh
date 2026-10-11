#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
PATCH_APPLICATION_DIGEST=${2:-}
ZIG_STORE_DIGEST=${3:-}

if [ -z "$SOURCE_DIR" ] || [ -z "$PATCH_APPLICATION_DIGEST" ] || [ -z "$ZIG_STORE_DIGEST" ]; then
    echo "Usage: $0 <source_dir> <patch_application_digest> <zig_store_digest>" >&2
    exit 1
fi

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source WindowsVtBuild.env

STAGE_ROOT="$ROOT_DIR/build/windows-vt-stage"
STAGE="$STAGE_ROOT/$GHOSTTY_WINDOWS_VT_LAYOUT"
METADATA="$STAGE_ROOT/$GHOSTTY_WINDOWS_VT_LAYOUT.version"
PREFIX="$STAGE_ROOT/prefix"
CACHE_ROOT="${BUILD_CACHE_ROOT:-$ROOT_DIR/build/cache}"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE/lib" "$STAGE/include"

OPTIONS=(
    "emit-lib-vt=true"
    "target=$GHOSTTY_WINDOWS_VT_TARGET"
    "cpu=$GHOSTTY_WINDOWS_VT_CPU"
    "optimize=$GHOSTTY_WINDOWS_VT_OPTIMIZE"
    "strip=true"
)

echo "[*] building Ghostty VT library for $GHOSTTY_WINDOWS_VT_TARGET"
(
    cd "$SOURCE_DIR"
    env \
        ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$CACHE_ROOT/zig-global}" \
        ZIG_LOCAL_CACHE_DIR="$CACHE_ROOT/vt/$GHOSTTY_WINDOWS_VT_TARGET/zig-local" \
        zig build "${OPTIONS[@]/#/-D}" --system "$SOURCE_DIR/zig-pkg" --prefix "$PREFIX"
)

ARCHIVE="$PREFIX/lib/ghostty-vt-static.lib"
if [ ! -f "$ARCHIVE" ]; then
    echo "[!] expected Ghostty VT static library not found: $ARCHIVE" >&2
    find "$PREFIX" -maxdepth 4 -type f -print | sort >&2
    exit 1
fi
cp "$ARCHIVE" "$STAGE/lib/"
cp -R "$PREFIX/include/." "$STAGE/include/"
rm -rf "$PREFIX"

# Every platform's library must expose the same C interface as the bindings.
if ! diff -r "$STAGE/include" "$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64/include" >/dev/null; then
    echo "[!] Windows Ghostty VT headers differ from the other platforms" >&2
    exit 1
fi

ARCHIVE_DIGEST=$(shasum -a 256 "$STAGE/lib/ghostty-vt-static.lib" | awk '{print $1}')
cat > "$METADATA" <<METADATA
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$(tr -d '[:space:]' < WindowsGhostty.ref)
patches_sha256=$("$ROOT_DIR/Script/artifact-digest.sh" Patches/ghostty)
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_store_sha256=$ZIG_STORE_DIGEST
ghostty_build_options=$(IFS=,; printf '%s' "${OPTIONS[*]}")
vt_target=$GHOSTTY_WINDOWS_VT_TARGET
vt_cpu=$GHOSTTY_WINDOWS_VT_CPU
vt_optimize=$GHOSTTY_WINDOWS_VT_OPTIMIZE
vt_archive_sha256=$ARCHIVE_DIGEST
METADATA

DESTINATION="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_WINDOWS_VT_LAYOUT"
rm -rf "$DESTINATION" "$DESTINATION.version"
mv "$STAGE" "$DESTINATION"
mv "$METADATA" "$DESTINATION.version"
rm -rf "$STAGE_ROOT"

LOCK_KEY="${GHOSTTY_WINDOWS_VT_LAYOUT/windows-/windows_}_vt_archive_sha256"
echo "[*] Windows VT: Vendor/GhosttyVt/$GHOSTTY_WINDOWS_VT_LAYOUT"
echo "[*] record in Ghostty.lock: $LOCK_KEY=$ARCHIVE_DIGEST"
