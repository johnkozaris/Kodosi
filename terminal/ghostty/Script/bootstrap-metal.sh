#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source Toolchain.env

ACTION=${1:-mount}
TOOLCHAIN_ROOT="$PWD/.tools/metal-$METAL_TOOLCHAIN_BUILD"
DOWNLOAD_ROOT="$TOOLCHAIN_ROOT/download"
MOUNT_POINT="$TOOLCHAIN_ROOT/mount"

unmount_toolchain() {
    if mount | grep -Fq " on $MOUNT_POINT "; then
        hdiutil detach "$MOUNT_POINT" >/dev/null
    fi
    rm -rf "$MOUNT_POINT"
}

case "$ACTION" in
    unmount)
        unmount_toolchain
        exit 0
        ;;
    mount) ;;
    *)
        echo "Usage: $0 [mount|unmount]" >&2
        exit 1
        ;;
esac

DMG=$(find "$DOWNLOAD_ROOT" -path '*/Restore/*.dmg' -type f -print -quit 2>/dev/null || true)
if [ -z "$DMG" ]; then
    echo "[*] downloading isolated Metal toolchain $METAL_TOOLCHAIN_BUILD" >&2
    rm -rf "$DOWNLOAD_ROOT"
    mkdir -p "$DOWNLOAD_ROOT"
    xcodebuild \
        -downloadComponent MetalToolchain \
        -exportPath "$DOWNLOAD_ROOT" \
        -buildVersion "$METAL_TOOLCHAIN_BUILD" >&2
    DMG=$(find "$DOWNLOAD_ROOT" -path '*/Restore/*.dmg' -type f -print -quit)
fi

printf '%s  %s\n' "$METAL_TOOLCHAIN_DMG_SHA256" "$DMG" |
    shasum -a 256 -c - >&2

unmount_toolchain
mkdir -p "$MOUNT_POINT"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT_POINT" "$DMG" >/dev/null

if [ ! -x "$MOUNT_POINT/Metal.xctoolchain/usr/bin/metal" ] ||
    [ ! -x "$MOUNT_POINT/Metal.xctoolchain/usr/bin/metallib" ]; then
    unmount_toolchain
    echo "[!] mounted Metal toolchain is incomplete" >&2
    exit 1
fi

printf '%s\n' "$MOUNT_POINT/Metal.xctoolchain"
