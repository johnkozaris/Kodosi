#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
PATCH_APPLICATION_DIGEST=${2:-}
ZIG_STORE_DIGEST=${3:-}
SKIP_TESTS=${4:-0}

if [ -z "$SOURCE_DIR" ] ||
    [ -z "$PATCH_APPLICATION_DIGEST" ] ||
    [ -z "$ZIG_STORE_DIGEST" ]; then
    echo "Usage: $0 <source_dir> <patch_application_digest> <zig_store_digest> [skip_tests]" >&2
    exit 1
fi
if ! command -v zig >/dev/null 2>&1; then
    echo "[!] zig not found" >&2
    exit 1
fi

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source AndroidVtBuild.env
NDK=$("$ROOT_DIR/scripts/find-android-ndk.sh")
NDK_BIN=$("$ROOT_DIR/scripts/find-android-ndk.sh" bin)

STAGE_ROOT="$ROOT_DIR/build/android-vt-stage"
ARTIFACT_STAGE="$STAGE_ROOT/GhosttyVt/$GHOSTTY_ANDROID_VT_LAYOUT"
METADATA_STAGE="$STAGE_ROOT/GhosttyVt/$GHOSTTY_ANDROID_VT_LAYOUT.version"
PREFIX="$STAGE_ROOT/prefix"
CACHE_ROOT="$ROOT_DIR/build/cache"
LOCAL_CACHE_DIR="$CACHE_ROOT/vt/$GHOSTTY_ANDROID_VT_TARGET/zig-local"
rm -rf "$STAGE_ROOT" "$LOCAL_CACHE_DIR"
mkdir -p "$ARTIFACT_STAGE/lib" "$ARTIFACT_STAGE/include" "$ARTIFACT_STAGE/symbols"

COMMAND=(zig build)
while IFS= read -r option; do
    COMMAND+=("$option")
done < <("$ROOT_DIR/scripts/android-vt-build-options.sh" arguments)
COMMAND+=(--system "$SOURCE_DIR/zig-pkg" --prefix "$PREFIX")

echo "[*] building Ghostty VT library for $GHOSTTY_ANDROID_VT_TARGET with NDK $GHOSTTY_ANDROID_NDK_VERSION"
(
    cd "$SOURCE_DIR"
    env \
        ANDROID_NDK_HOME="$NDK" \
        ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$CACHE_ROOT/zig-global}" \
        ZIG_LOCAL_CACHE_DIR="$LOCAL_CACHE_DIR" \
        "${COMMAND[@]}"
)

ARCHIVE="$PREFIX/lib/libghostty-vt.a"
if [ ! -f "$ARCHIVE" ] || [ ! -f "$PREFIX/include/ghostty/vt.h" ]; then
    echo "[!] the Ghostty VT build for Android is incomplete" >&2
    find "$PREFIX" -maxdepth 4 -type f -print | sort >&2
    exit 1
fi

python3 - "$ARCHIVE" "$ARTIFACT_STAGE/lib/libghostty-vt.a" "$STAGE_ROOT/objects" "$NDK_BIN" "$ROOT_DIR" "$NDK" <<'PY'
import subprocess
import sys
from pathlib import Path

source, destination, object_root, tools = (Path(argument) for argument in sys.argv[1:5])
build_paths = sys.argv[5:7]
ar, strip = tools / "llvm-ar", tools / "llvm-strip"
object_root.mkdir(parents=True)

members = [Path(member).name for member in subprocess.check_output([ar, "t", source], text=True).splitlines()]
if len(members) != len(set(members)):
    raise SystemExit("[!] Ghostty VT archive contains duplicate object basenames")
subprocess.run([ar, "x", "--output", object_root, source], check=True)
objects = sorted(path.name for path in object_root.iterdir())
if objects != sorted(members):
    raise SystemExit("[!] Ghostty VT archive extraction changed its member set")
for name in objects:
    path = object_root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"[!] Ghostty VT archive contains an empty object: {name}")
    subprocess.run([strip, "--strip-debug", path], check=True)
subprocess.run([ar, "rcsD", destination, *(object_root / name for name in objects)], check=True)
if subprocess.check_output([ar, "t", destination], text=True).splitlines() != objects:
    raise SystemExit("[!] normalized Ghostty VT archive changed its object set")
if any(path.encode() in destination.read_bytes() for path in build_paths):
    raise SystemExit("[!] Ghostty VT archive embeds the path of the repository or the NDK")
PY
rm -rf "$STAGE_ROOT/objects"
cp -R "$PREFIX/include/." "$ARTIFACT_STAGE/include/"
rm -rf "$PREFIX"

if ! diff -r "$ARTIFACT_STAGE/include" "$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64/include" >/dev/null; then
    echo "[!] Android Ghostty VT headers differ from the other platforms" >&2
    exit 1
fi
"$NDK_BIN/llvm-nm" -g --defined-only "$ARTIFACT_STAGE/lib/libghostty-vt.a" |
    awk '$2 ~ /^[A-Z]$/ {print $3}' |
    LC_ALL=C sort -u > "$ARTIFACT_STAGE/symbols/strong.txt"

PATCHES_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/ghostty)
ARTIFACT_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" artifact "$ARTIFACT_STAGE")
ARCHIVE_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" file "$ARTIFACT_STAGE/lib/libghostty-vt.a")
BUILD_OPTIONS=$("$ROOT_DIR/scripts/android-vt-build-options.sh" vector)

cat > "$METADATA_STAGE" <<EOF
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$(tr -d '[:space:]' < AndroidGhostty.ref)
patches_sha256=$PATCHES_DIGEST
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_patches_sha256=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/zig)
zig_store_sha256=$ZIG_STORE_DIGEST
ghostty_build_options=$BUILD_OPTIONS
vt_target=$GHOSTTY_ANDROID_VT_TARGET
vt_cpu=$GHOSTTY_ANDROID_VT_CPU
vt_optimize=$GHOSTTY_ANDROID_VT_OPTIMIZE
ndk_version=$GHOSTTY_ANDROID_NDK_VERSION
vt_archive_sha256=$ARCHIVE_DIGEST
vt_artifact_sha256=$ARTIFACT_DIGEST
EOF

if [ "$SKIP_TESTS" -eq 0 ]; then
    python3 "$ROOT_DIR/scripts/verify-ghostty-pins.py" \
        --working-tree --vendor "$STAGE_ROOT" --platform android --patched-source "$SOURCE_DIR"
    "$ROOT_DIR/scripts/verify-android-vt-artifact.sh" --vendor "$STAGE_ROOT"
    python3 "$ROOT_DIR/scripts/verify-vt-third-party-notices.py" --vendor "$STAGE_ROOT" --layout android-arm64
fi

DESTINATION="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_ANDROID_VT_LAYOUT"
rm -rf "$DESTINATION" "$DESTINATION.version"
mv "$ARTIFACT_STAGE" "$DESTINATION"
mv "$METADATA_STAGE" "$DESTINATION.version"
rm -rf "$STAGE_ROOT"

LOCK_KEY="${GHOSTTY_ANDROID_VT_LAYOUT//-/_}_archive_sha256"
echo "[*] Android VT: Vendor/GhosttyVt/$GHOSTTY_ANDROID_VT_LAYOUT"
echo "[*] record in Ghostty.lock: $LOCK_KEY=$ARCHIVE_DIGEST"
