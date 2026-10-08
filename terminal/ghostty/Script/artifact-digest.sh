#!/bin/bash

set -euo pipefail

ARTIFACT_PATH=${1:-Vendor/GhosttyKit.xcframework}

if [ ! -d "$ARTIFACT_PATH" ]; then
    echo "[!] artifact not found: $ARTIFACT_PATH" >&2
    exit 1
fi

(
    cd "$ARTIFACT_PATH"
    find . -type f -print |
        LC_ALL=C sort |
        while IFS= read -r file; do
            shasum -a 256 "$file"
        done
) | shasum -a 256 | awk '{print $1}'
