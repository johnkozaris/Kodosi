#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=all)" ]]; then
    echo "Commit the Kodosi source before building a release." >&2
    exit 1
fi
echo "Verified clean Kodosi source $(git -C "$ROOT" rev-parse HEAD)."
