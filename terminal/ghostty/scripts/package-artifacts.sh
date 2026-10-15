#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

if [ $# -ne 4 ]; then
    echo "Usage: $0 <staged_dir> <renderer_output> <vt_output> <symbol_dir>" >&2
    exit 1
fi
STAGED_DIR=$1
RENDERER_OUTPUT=$2
VT_OUTPUT=$3
SYMBOL_DIR=$4

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

for slice in "${SLICES[@]}"; do
    if ! cmp -s "$STAGED_DIR/$slice/lib/libghostty.a" "$RENDERER_OUTPUT/$slice/libghostty.a"; then
        echo "[!] packaged $slice archive differs from the built image"
        exit 1
    fi
done

mkdir -p "$SYMBOL_DIR"
for slice in "${SLICES[@]}"; do
    manifest="$SYMBOL_DIR/$slice-strong.txt"
    if [ "$slice" = macos-arm64 ]; then
        manifest="$SYMBOL_DIR/combined-strong.txt"
    fi
    nm -gU "$RENDERER_OUTPUT/$slice/libghostty.a" | awk '$2 ~ /^[A-Z]$/ {print $3}' | LC_ALL=C sort > "$manifest"
done

echo "[*] packaged one arm64 Ghostty image per Apple slice for renderer and VT consumers"
