#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

STAGED_DIR=${1:-}
RENDERER_OUTPUT=${2:-Vendor/GhosttyKit.xcframework}
VT_OUTPUT=${3:-Vendor/GhosttyVt/macos-arm64}
SYMBOL_DIR=${4:-Vendor/symbols}

if [ -z "$STAGED_DIR" ]; then
    echo "Usage: $0 <staged_dir> [renderer_output] [vt_output] [symbol_dir]"
    exit 1
fi

STAGE="$STAGED_DIR/macos-arm64"
COMBINED_ARCHIVE="$STAGE/lib/libghostty.a"
RENDERER_HEADERS="$STAGE/include/renderer"
VT_HEADERS="$STAGE/include/vt"

for path in \
    "$COMBINED_ARCHIVE" \
    "$RENDERER_HEADERS/ghostty.h" \
    "$RENDERER_HEADERS/module.modulemap" \
    "$VT_HEADERS/ghostty/vt.h" \
    "$VT_HEADERS/module.modulemap"; do
    if [ ! -e "$path" ]; then
        echo "[!] missing staged Ghostty artifact: $path"
        exit 1
    fi
done

if [ "$(lipo -archs "$COMBINED_ARCHIVE")" != arm64 ]; then
    echo "[!] combined Ghostty artifact must contain only arm64"
    exit 1
fi

rm -rf "$RENDERER_OUTPUT" "$VT_OUTPUT"
mkdir -p "$(dirname "$RENDERER_OUTPUT")" "$VT_OUTPUT/lib" "$VT_OUTPUT/include"
xcodebuild -create-xcframework \
    -library "$COMBINED_ARCHIVE" \
    -headers "$RENDERER_HEADERS" \
    -output "$RENDERER_OUTPUT"
cp "$COMBINED_ARCHIVE" "$VT_OUTPUT/lib/libghostty-vt.a"
cp -R "$VT_HEADERS/." "$VT_OUTPUT/include/"

./Script/verify-xcframework.sh "$RENDERER_OUTPUT"

if ! grep -Fq 'module GhosttyVt' "$VT_OUTPUT/include/module.modulemap"; then
    echo "[!] VT module map does not declare GhosttyVt"
    exit 1
fi

PACKAGED_RENDERER_ARCHIVE=$(find "$RENDERER_OUTPUT" -name libghostty.a -type f -print -quit)
if ! cmp -s "$PACKAGED_RENDERER_ARCHIVE" "$VT_OUTPUT/lib/libghostty-vt.a"; then
    echo "[!] renderer and VT artifact paths do not contain the same native image"
    exit 1
fi

mkdir -p "$SYMBOL_DIR"
DUPLICATE_SYMBOLS=$(
    nm -gU "$COMBINED_ARCHIVE" |
        awk '$2 ~ /^[A-Z]$/ {counts[$3]++} END {for (symbol in counts) if (counts[symbol] > 1) print symbol}' |
        LC_ALL=C sort
)
if [ -n "$DUPLICATE_SYMBOLS" ]; then
    echo "[!] combined Ghostty archive defines duplicate strong symbols"
    printf '%s\n' "$DUPLICATE_SYMBOLS"
    exit 1
fi
nm -gU "$COMBINED_ARCHIVE" | awk '$2 ~ /^[A-Z]$/ {print $3}' | LC_ALL=C sort > "$SYMBOL_DIR/combined-strong.txt"
rm -f \
    "$SYMBOL_DIR/renderer-strong.txt" \
    "$SYMBOL_DIR/vt-strong.txt" \
    "$SYMBOL_DIR/strong-intersection.txt"

echo "[*] packaged one arm64 Ghostty image for renderer and VT consumers"
