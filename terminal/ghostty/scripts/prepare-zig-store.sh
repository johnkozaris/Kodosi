#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

SOURCE_DIR=${1:-}
if [ -z "$SOURCE_DIR" ] || [ ! -f "$SOURCE_DIR/build.zig.zon" ]; then
    echo "Usage: $0 <patched_source_dir>"
    exit 1
fi

if ! command -v zig >/dev/null 2>&1; then
    echo "[!] zig not found"
    exit 1
fi

rm -rf "$SOURCE_DIR/zig-pkg"
(
    cd "$SOURCE_DIR"
    zig build --fetch=all
)

if [ ! -d "$SOURCE_DIR/zig-pkg" ]; then
    echo "[!] Zig package store was not materialized"
    exit 1
fi

"$PWD/Script/tree-digest.sh" "$SOURCE_DIR/zig-pkg"
