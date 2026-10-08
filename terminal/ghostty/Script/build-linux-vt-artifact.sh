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
ARTIFACT_STAGE="$STAGE_ROOT/$GHOSTTY_LINUX_VT_LAYOUT"
METADATA_STAGE="$STAGE_ROOT/$GHOSTTY_LINUX_VT_LAYOUT.version"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE_ROOT"

if [ "$SKIP_TESTS" -eq 0 ]; then
    "$ROOT_DIR/Script/verify-linux-vt-source.sh" "$SOURCE_DIR"
fi

"$ROOT_DIR/Script/build-ghostty-vt.sh" "$SOURCE_DIR" "$ARTIFACT_STAGE"

PATCHES_DIGEST=$("$ROOT_DIR/Script/artifact-digest.sh" Patches/ghostty)
ARTIFACT_DIGEST=$("$ROOT_DIR/Script/linux-artifact-digest.sh" "$ARTIFACT_STAGE")
ARCHIVE_DIGEST=$(sha256sum "$ARTIFACT_STAGE/lib/libghostty-vt.a" | awk '{print $1}')
BUILD_OPTIONS=$("$ROOT_DIR/Script/linux-vt-build-options.sh" vector)

cat > "$METADATA_STAGE" <<EOF
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$(tr -d '[:space:]' < LinuxGhostty.ref)
patches_sha256=$PATCHES_DIGEST
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_store_sha256=$ZIG_STORE_DIGEST
ghostty_build_options=$BUILD_OPTIONS
vt_target=$GHOSTTY_LINUX_VT_TARGET
vt_cpu=$GHOSTTY_LINUX_VT_CPU
vt_optimize=$GHOSTTY_LINUX_VT_OPTIMIZE
vt_archive_sha256=$ARCHIVE_DIGEST
vt_artifact_sha256=$ARTIFACT_DIGEST
EOF

if [ "$SKIP_TESTS" -eq 0 ]; then
    KODOSI_GHOSTTY_REQUIRE_BUILD_PROVENANCE=1 \
        KODOSI_GHOSTTY_PATCHED_SOURCE="$SOURCE_DIR" \
        KODOSI_GHOSTTY_LINUX_ARTIFACT_ROOT="$ARTIFACT_STAGE" \
        KODOSI_GHOSTTY_LINUX_METADATA="$METADATA_STAGE" \
        "$ROOT_DIR/Script/verify-linux-vt-artifact.sh"
    KODOSI_GHOSTTY_LINUX_ARTIFACT_ROOT="$ARTIFACT_STAGE" \
        KODOSI_GHOSTTY_LINUX_METADATA="$METADATA_STAGE" \
        "$ROOT_DIR/Script/verify-linux-third-party-notices.py"
fi

DESTINATION="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"
DESTINATION_METADATA="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT.version"
BACKUP="$ROOT_DIR/build/$GHOSTTY_LINUX_VT_LAYOUT.previous"
BACKUP_METADATA="$ROOT_DIR/build/$GHOSTTY_LINUX_VT_LAYOUT.version.previous"
rm -rf "$BACKUP"
rm -f "$BACKUP_METADATA"
HAD_ARTIFACT=0
HAD_METADATA=0
rollback_promotion() {
    rm -rf "$DESTINATION"
    rm -f "$DESTINATION_METADATA"
    if [ "$HAD_ARTIFACT" -eq 1 ] && [ -e "$BACKUP" ]; then
        mv "$BACKUP" "$DESTINATION"
    fi
    if [ "$HAD_METADATA" -eq 1 ] && [ -e "$BACKUP_METADATA" ]; then
        mv "$BACKUP_METADATA" "$DESTINATION_METADATA"
    fi
}
if [ -e "$DESTINATION" ]; then
    mv "$DESTINATION" "$BACKUP"
    HAD_ARTIFACT=1
fi
if [ -e "$DESTINATION_METADATA" ]; then
    if ! mv "$DESTINATION_METADATA" "$BACKUP_METADATA"; then
        rollback_promotion
        exit 1
    fi
    HAD_METADATA=1
fi
if ! mv "$ARTIFACT_STAGE" "$DESTINATION"; then
    rollback_promotion
    exit 1
fi
if ! mv "$METADATA_STAGE" "$DESTINATION_METADATA"; then
    rollback_promotion
    exit 1
fi

if ! "$ROOT_DIR/Script/verify-linux-vt-artifact.sh"; then
    rollback_promotion
    exit 1
fi
if ! "$ROOT_DIR/Script/verify-linux-third-party-notices.py"; then
    rollback_promotion
    exit 1
fi
rm -rf "$BACKUP"
rm -f "$BACKUP_METADATA"
echo "[*] Linux VT: Vendor/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"
