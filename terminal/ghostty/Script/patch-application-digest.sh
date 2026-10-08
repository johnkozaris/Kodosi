#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

SOURCE_DIR=${1:-}
EXPECTED_COMMIT=${2:-}

if [ -z "$SOURCE_DIR" ] || [ -z "$EXPECTED_COMMIT" ]; then
    echo "Usage: $0 <source_dir> <expected_commit>" >&2
    exit 1
fi

if [ ! -d "$SOURCE_DIR/.git" ]; then
    echo "[!] patched source is not a Git worktree: $SOURCE_DIR" >&2
    exit 1
fi

if [ "$(git -C "$SOURCE_DIR" rev-parse HEAD)" != "$EXPECTED_COMMIT" ]; then
    echo "[!] patched source does not use the expected Ghostty commit" >&2
    exit 1
fi

TEMP_INDEX=$(mktemp "${TMPDIR:-/tmp}/kodosi-ghostty-index.XXXXXX")
rm -f "$TEMP_INDEX"
trap 'rm -f "$TEMP_INDEX"' EXIT

GIT_INDEX_FILE="$TEMP_INDEX" git -C "$SOURCE_DIR" read-tree HEAD
GIT_INDEX_FILE="$TEMP_INDEX" git -C "$SOURCE_DIR" add --all
if GIT_INDEX_FILE="$TEMP_INDEX" git -C "$SOURCE_DIR" diff --cached --quiet HEAD; then
    echo "[!] patched source has no patch application" >&2
    exit 1
fi

GIT_INDEX_FILE="$TEMP_INDEX" git -C "$SOURCE_DIR" diff \
    --cached \
    --binary \
    --full-index \
    HEAD |
    shasum -a 256 |
    awk '{print $1}'
