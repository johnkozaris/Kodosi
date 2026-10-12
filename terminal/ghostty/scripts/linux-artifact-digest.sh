#!/bin/bash

set -euo pipefail

ARTIFACT_PATH=${1:-}
if [ -z "$ARTIFACT_PATH" ] || [ ! -d "$ARTIFACT_PATH" ]; then
    echo "Usage: $0 <artifact_directory>" >&2
    exit 1
fi

(
    cd "$ARTIFACT_PATH"
    find . \( -type f -o -type l \) -print |
        LC_ALL=C sort |
        while IFS= read -r path; do
            if [ -L "$path" ]; then
                printf 'symlink  %s -> %s\n' "$path" "$(readlink "$path")"
            else
                sha256sum "$path"
            fi
        done
) | sha256sum | awk '{print $1}'
