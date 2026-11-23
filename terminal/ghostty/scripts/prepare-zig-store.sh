#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

SOURCE_DIR=${1:-}
if [ -z "$SOURCE_DIR" ] || [ ! -f "$SOURCE_DIR/build.zig.zon" ]; then
    echo "Usage: $0 <patched_source_dir> [zig build option...]"
    exit 1
fi
shift

if ! command -v zig >/dev/null 2>&1; then
    echo "[!] zig not found"
    exit 1
fi

rm -rf "$SOURCE_DIR/zig-pkg"
# Zig 0.17 can crash a --system build whose global cache holds package
# archives, and a build cache without them proves that the build used only
# the store, so the fetch keeps its own cache apart from the build cache.
FETCH_CACHE_DIR="$PWD/build/cache/zig-fetch"
FETCH=(--fetch=all)
if [ $# -gt 0 ]; then
    # Configuring the build with these options fetches the lazy packages that it asks for.
    # Zig also puts each lazy package that its cache already holds into the store, so this
    # fetch starts from an empty cache.
    FETCH_CACHE_DIR="$PWD/build/cache/zig-fetch-build"
    rm -rf "$FETCH_CACHE_DIR"
    FETCH=(--list-steps "$@")
fi
(
    cd "$SOURCE_DIR"
    ZIG_GLOBAL_CACHE_DIR="$FETCH_CACHE_DIR" zig build "${FETCH[@]}" >/dev/null
)

if [ ! -d "$SOURCE_DIR/zig-pkg" ]; then
    echo "[!] Zig package store was not materialized"
    exit 1
fi

python3 "$PWD/scripts/digest.py" tree "$SOURCE_DIR/zig-pkg"
