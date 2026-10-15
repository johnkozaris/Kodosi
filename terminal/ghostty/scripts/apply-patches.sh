#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

SOURCE_DIR=${1:-}
if [ -z "$SOURCE_DIR" ]; then
    echo "Usage: $0 <source_dir>" >&2
    exit 1
fi

for patch_file in "$PWD"/Patches/ghostty/*; do
    case "$patch_file" in
        *.md) ;;
        *.patch)
            git -C "$SOURCE_DIR" apply "$patch_file"
            echo "[+] applied patch: $(basename "$patch_file")"
            ;;
        *)
            echo "[-] unsupported patch file: $patch_file" >&2
            exit 1
            ;;
    esac
done
