#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
ZIG_TARGET=${2:-}
OUTPUT_DIR=${3:-}

if [ -z "$SOURCE_DIR" ] || [ -z "$ZIG_TARGET" ] || [ -z "$OUTPUT_DIR" ]; then
    echo "Usage: $0 <source_dir> <zig_target> <output_dir>"
    exit 1
fi

if [ ! -d "$SOURCE_DIR" ]; then
    echo "[!] ghostty source directory not found: $SOURCE_DIR"
    exit 1
fi

# shellcheck disable=SC1091
source NativeBuild.env
if [ "$ZIG_TARGET" != "$GHOSTTY_BUILD_TARGET" ]; then
    echo "[!] unsupported Ghostty target: $ZIG_TARGET"
    exit 1
fi

if ! command -v zig >/dev/null 2>&1; then
    echo "[!] zig not found"
    exit 1
fi

CACHE_ROOT="${BUILD_CACHE_ROOT:-$ROOT_DIR/build/cache}"
GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$CACHE_ROOT/zig-global}"
LOCAL_CACHE_DIR="$CACHE_ROOT/combined/$ZIG_TARGET/zig-local"
MODULE_CACHE_DIR="${CLANG_MODULE_CACHE_ROOT:-$CACHE_ROOT/clang-module-cache}/combined/$ZIG_TARGET"
PREFIX="$OUTPUT_DIR/.prefix"

rm -rf "$OUTPUT_DIR" "$LOCAL_CACHE_DIR" "$MODULE_CACHE_DIR"
mkdir -p \
    "$OUTPUT_DIR/lib" \
    "$OUTPUT_DIR/include/renderer" \
    "$OUTPUT_DIR/include/vt/ghostty" \
    "$GLOBAL_CACHE_DIR" \
    "$LOCAL_CACHE_DIR" \
    "$MODULE_CACHE_DIR"

COMMAND=(zig build)
while IFS= read -r option; do
    COMMAND+=("$option")
done < <("$ROOT_DIR/Script/native-build-options.sh" arguments)
COMMAND+=(
    --system "$SOURCE_DIR/zig-pkg"
    --prefix "$PREFIX"
)
LIBRARY="$PREFIX/lib/libghostty.a"

echo "[*] building combined Ghostty renderer and VT static library"
echo "    target: $ZIG_TARGET"
echo "    cpu: $GHOSTTY_BUILD_CPU"
echo "    source: $SOURCE_DIR"

(
    cd "$SOURCE_DIR"
    env \
        CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" \
        ZIG_GLOBAL_CACHE_DIR="$GLOBAL_CACHE_DIR" \
        ZIG_LOCAL_CACHE_DIR="$LOCAL_CACHE_DIR" \
        "${COMMAND[@]}"
)

if [ ! -f "$LIBRARY" ]; then
    echo "[!] expected combined Ghostty archive not found: $LIBRARY"
    find "$PREFIX" -maxdepth 4 -type f | sort
    exit 1
fi

cp "$LIBRARY" "$OUTPUT_DIR/lib/libghostty.a"
cp "$SOURCE_DIR/include/ghostty.h" "$OUTPUT_DIR/include/renderer/ghostty.h"
cat > "$OUTPUT_DIR/include/renderer/module.modulemap" <<'MAP'
module libghostty {
    umbrella header "ghostty.h"
    export *
}
MAP
cp "$SOURCE_DIR/include/ghostty/vt.h" "$OUTPUT_DIR/include/vt/ghostty/vt.h"
cp -R "$SOURCE_DIR/include/ghostty/vt" "$OUTPUT_DIR/include/vt/ghostty/vt"
cat > "$OUTPUT_DIR/include/vt/module.modulemap" <<'MAP'
module GhosttyVt {
    umbrella header "ghostty/vt.h"
    export *
}
MAP

rm -rf "$PREFIX"
echo "[*] built combined Ghostty archive: $OUTPUT_DIR/lib/libghostty.a"
