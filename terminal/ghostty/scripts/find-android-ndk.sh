#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source AndroidVtBuild.env

NDK=${ANDROID_NDK_HOME:-${ANDROID_HOME:-$HOME/Library/Android/sdk}/ndk/$GHOSTTY_ANDROID_NDK_VERSION}
REVISION=$(sed -n 's/^Pkg.Revision *= *//p' "$NDK/source.properties" 2>/dev/null || true)
if [ "$REVISION" != "$GHOSTTY_ANDROID_NDK_VERSION" ]; then
    echo "[!] Android NDK $GHOSTTY_ANDROID_NDK_VERSION not found at $NDK; install it with sdkmanager" >&2
    exit 1
fi
case "$(uname -s)" in
    Darwin) HOST=darwin-x86_64 ;;
    Linux) HOST=linux-x86_64 ;;
    *)
        echo "[!] the Android NDK builds on macOS or Linux, not $(uname -s)" >&2
        exit 1
        ;;
esac

case "${1:-root}" in
    root) printf '%s\n' "$NDK" ;;
    bin) printf '%s\n' "$NDK/toolchains/llvm/prebuilt/$HOST/bin" ;;
    *)
        echo "Usage: $0 [root|bin]" >&2
        exit 1
        ;;
esac
