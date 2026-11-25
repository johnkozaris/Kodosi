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
# Isolate the fetch cache from the build cache: Zig 0.17 can crash a --system build if its
# global cache holds package archives, and keeping them separate ensures builds only use zig-pkg.
FETCH_CACHE_DIR="$PWD/build/cache/zig-fetch"
FETCH=(--fetch=all)
if [ $# -gt 0 ]; then
    # --list-steps fetches only the lazy dependencies needed for these build options. Clear the
    # cache first so Zig does not also copy unrelated cached packages into zig-pkg.
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
