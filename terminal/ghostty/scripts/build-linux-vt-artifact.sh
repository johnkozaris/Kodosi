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

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source LinuxVtBuild.env

STAGE_ROOT="$ROOT_DIR/build/linux-vt-stage"
ARTIFACT_STAGE="$STAGE_ROOT/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"
METADATA_STAGE="$STAGE_ROOT/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT.version"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE_ROOT/GhosttyVt"

if [ "$SKIP_TESTS" -eq 0 ]; then
    "$ROOT_DIR/scripts/verify-linux-vt-source.sh" "$SOURCE_DIR"
fi

"$ROOT_DIR/scripts/build-ghostty-vt.sh" "$SOURCE_DIR" "$ARTIFACT_STAGE"

PATCHES_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/ghostty)
ARTIFACT_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" artifact "$ARTIFACT_STAGE")
ARCHIVE_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" file "$ARTIFACT_STAGE/lib/libghostty-vt.a")
BUILD_OPTIONS=$("$ROOT_DIR/scripts/linux-vt-build-options.sh" vector)

cat > "$METADATA_STAGE" <<EOF
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$(tr -d '[:space:]' < LinuxGhostty.ref)
patches_sha256=$PATCHES_DIGEST
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_patches_sha256=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/zig)
zig_store_sha256=$ZIG_STORE_DIGEST
ghostty_build_options=$BUILD_OPTIONS
vt_target=$GHOSTTY_LINUX_VT_TARGET
vt_cpu=$GHOSTTY_LINUX_VT_CPU
vt_optimize=$GHOSTTY_LINUX_VT_OPTIMIZE
vt_archive_sha256=$ARCHIVE_DIGEST
vt_artifact_sha256=$ARTIFACT_DIGEST
EOF

if [ "$SKIP_TESTS" -eq 0 ]; then
    python3 "$ROOT_DIR/scripts/verify-ghostty-pins.py" \
        --working-tree --vendor "$STAGE_ROOT" --platform linux --patched-source "$SOURCE_DIR"
    "$ROOT_DIR/scripts/verify-linux-vt-artifact.sh" --vendor "$STAGE_ROOT"
    python3 "$ROOT_DIR/scripts/verify-linux-third-party-notices.py" --vendor "$STAGE_ROOT"
fi

DESTINATION="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"
rm -rf "$DESTINATION" "$DESTINATION.version"
mv "$ARTIFACT_STAGE" "$DESTINATION"
mv "$METADATA_STAGE" "$DESTINATION.version"
echo "[*] Linux VT: Vendor/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"
