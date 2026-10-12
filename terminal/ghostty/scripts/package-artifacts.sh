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

SLICES=()
while IFS= read -r slice; do
    SLICES+=("$slice")
done < <(./scripts/native-build-options.sh slices)

XCFRAMEWORK_ARGUMENTS=()
for slice in "${SLICES[@]}"; do
    stage="$STAGED_DIR/$slice"
    for path in \
        "$stage/lib/libghostty.a" \
        "$stage/include/renderer/ghostty.h" \
        "$stage/include/renderer/module.modulemap" \
        "$stage/include/vt/ghostty/vt.h" \
        "$stage/include/vt/module.modulemap"; do
        if [ ! -e "$path" ]; then
            echo "[!] missing staged Ghostty artifact: $path"
            exit 1
        fi
    done
    if [ "$(lipo -archs "$stage/lib/libghostty.a")" != arm64 ]; then
        echo "[!] $slice Ghostty artifact must contain only arm64"
        exit 1
    fi
    if ! diff -r "$STAGED_DIR/macos-arm64/include" "$stage/include" >/dev/null; then
        echo "[!] $slice Ghostty headers differ from the macOS headers"
        exit 1
    fi
    XCFRAMEWORK_ARGUMENTS+=(
        -library "$stage/lib/libghostty.a"
        -headers "$stage/include/renderer"
    )
done

rm -rf "$RENDERER_OUTPUT" "$VT_OUTPUT"
mkdir -p "$(dirname "$RENDERER_OUTPUT")" "$VT_OUTPUT/lib" "$VT_OUTPUT/include"
xcodebuild -create-xcframework \
    "${XCFRAMEWORK_ARGUMENTS[@]}" \
    -output "$RENDERER_OUTPUT"
cp "$STAGED_DIR/macos-arm64/lib/libghostty.a" "$VT_OUTPUT/lib/libghostty-vt.a"
cp -R "$STAGED_DIR/macos-arm64/include/vt/." "$VT_OUTPUT/include/"

./scripts/verify-xcframework.sh "$RENDERER_OUTPUT"

if ! grep -Fq 'module GhosttyVt' "$VT_OUTPUT/include/module.modulemap"; then
    echo "[!] VT module map does not declare GhosttyVt"
    exit 1
fi

for slice in "${SLICES[@]}"; do
    if ! cmp -s "$STAGED_DIR/$slice/lib/libghostty.a" "$RENDERER_OUTPUT/$slice/libghostty.a"; then
        echo "[!] packaged $slice archive differs from the built image"
        exit 1
    fi
done
if ! cmp -s "$RENDERER_OUTPUT/macos-arm64/libghostty.a" "$VT_OUTPUT/lib/libghostty-vt.a"; then
    echo "[!] renderer and VT artifact paths do not contain the same native image"
    exit 1
fi

mkdir -p "$SYMBOL_DIR"
for slice in "${SLICES[@]}"; do
    archive="$RENDERER_OUTPUT/$slice/libghostty.a"
    manifest="$SYMBOL_DIR/$slice-strong.txt"
    if [ "$slice" = macos-arm64 ]; then
        manifest="$SYMBOL_DIR/combined-strong.txt"
    fi
    duplicate_symbols=$(
        nm -gU "$archive" |
            awk '$2 ~ /^[A-Z]$/ {counts[$3]++} END {for (symbol in counts) if (counts[symbol] > 1) print symbol}' |
            LC_ALL=C sort
    )
    if [ -n "$duplicate_symbols" ]; then
        echo "[!] $slice Ghostty archive defines duplicate strong symbols"
        printf '%s\n' "$duplicate_symbols"
        exit 1
    fi
    nm -gU "$archive" | awk '$2 ~ /^[A-Z]$/ {print $3}' | LC_ALL=C sort > "$manifest"
done

echo "[*] packaged one arm64 Ghostty image per Apple slice for renderer and VT consumers"
