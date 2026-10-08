#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
OUTPUT_DIR=${2:-}

if [ -z "$SOURCE_DIR" ] || [ -z "$OUTPUT_DIR" ]; then
    echo "Usage: $0 <source_dir> <output_dir>" >&2
    exit 1
fi
if [ ! -d "$SOURCE_DIR/zig-pkg" ]; then
    echo "[!] prepared Ghostty source is missing zig-pkg: $SOURCE_DIR" >&2
    exit 1
fi
if ! command -v zig >/dev/null 2>&1; then
    echo "[!] zig not found" >&2
    exit 1
fi

# shellcheck disable=SC1091
source LinuxVtBuild.env

CACHE_ROOT="${BUILD_CACHE_ROOT:-$ROOT_DIR/build/cache}"
GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$CACHE_ROOT/zig-global}"
LOCAL_CACHE_DIR="$CACHE_ROOT/vt/$GHOSTTY_LINUX_VT_TARGET/zig-local"
PREFIX="$OUTPUT_DIR/.prefix"

rm -rf "$OUTPUT_DIR" "$LOCAL_CACHE_DIR"
mkdir -p "$OUTPUT_DIR/lib" "$OUTPUT_DIR/include" "$GLOBAL_CACHE_DIR" "$LOCAL_CACHE_DIR"

COMMAND=(zig build)
while IFS= read -r option; do
    COMMAND+=("$option")
done < <("$ROOT_DIR/Script/linux-vt-build-options.sh" arguments)
COMMAND+=(
    --system "$SOURCE_DIR/zig-pkg"
    --prefix "$PREFIX"
)

echo "[*] building Ghostty VT library"
echo "    target: $GHOSTTY_LINUX_VT_TARGET"
echo "    cpu: $GHOSTTY_LINUX_VT_CPU"
echo "    source: $SOURCE_DIR"

(
    cd "$SOURCE_DIR"
    env \
        ZIG_GLOBAL_CACHE_DIR="$GLOBAL_CACHE_DIR" \
        ZIG_LOCAL_CACHE_DIR="$LOCAL_CACHE_DIR" \
        "${COMMAND[@]}"
)

ARCHIVE="$PREFIX/lib/libghostty-vt.a"
if [ ! -f "$ARCHIVE" ]; then
    echo "[!] expected Ghostty VT archive not found: $ARCHIVE" >&2
    find "$PREFIX" -maxdepth 4 -type f -print | sort
    exit 1
fi
if [ ! -f "$PREFIX/include/ghostty/vt.h" ]; then
    echo "[!] installed Ghostty VT headers are incomplete" >&2
    exit 1
fi

python3 - "$ARCHIVE" "$OUTPUT_DIR/lib/libghostty-vt.a" "$OUTPUT_DIR/.archive-objects" <<'PY'
import subprocess
import sys
from pathlib import Path

source = Path(sys.argv[1])
destination = Path(sys.argv[2])
object_root = Path(sys.argv[3])
object_root.mkdir(parents=True)

members = subprocess.check_output(["ar", "t", source], text=True).splitlines()
basenames = [Path(member).name for member in members]
if len(basenames) != len(set(basenames)):
    raise SystemExit("[!] Ghostty VT archive contains duplicate object basenames")

subprocess.run(
    ["ar", "x", "--output", object_root, source],
    check=True,
)
objects = sorted(object_root.iterdir())
if [path.name for path in objects] != sorted(basenames):
    raise SystemExit("[!] Ghostty VT archive extraction changed its member set")
if any(not path.is_file() or path.stat().st_size == 0 for path in objects):
    raise SystemExit("[!] Ghostty VT archive contains an empty object")
for path in objects:
    subprocess.run(["strip", "--strip-debug", path], check=True)

subprocess.run(
    ["ar", "crD", destination, *map(str, objects)],
    check=True,
)
subprocess.run(["ranlib", "-D", destination], check=True)
PY
rm -rf "$OUTPUT_DIR/.archive-objects"
cp -R "$PREFIX/include/." "$OUTPUT_DIR/include/"
if [ -f "$PREFIX/lib/libghostty-vt.so.0.1.0" ]; then
    cp "$PREFIX/lib/libghostty-vt.so.0.1.0" "$OUTPUT_DIR/lib/"
    strip --strip-unneeded "$OUTPUT_DIR/lib/libghostty-vt.so.0.1.0"
    ln -s libghostty-vt.so.0.1.0 "$OUTPUT_DIR/lib/libghostty-vt.so.0"
    ln -s libghostty-vt.so.0 "$OUTPUT_DIR/lib/libghostty-vt.so"
fi

mkdir -p "$OUTPUT_DIR/symbols"
nm -g --defined-only "$OUTPUT_DIR/lib/libghostty-vt.a" |
    awk '$2 ~ /^[A-Z]$/ {print $3}' |
    LC_ALL=C sort -u > "$OUTPUT_DIR/symbols/strong.txt"

rm -rf "$PREFIX"
echo "[*] built Ghostty VT artifact: $OUTPUT_DIR"
