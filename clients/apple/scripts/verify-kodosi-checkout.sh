#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/apple-platform.sh"
repository=$(kodosi_repository_root)
if [[ -n "$(git -C "$repository" status --porcelain --untracked-files=all)" ]]; then
    echo "Commit all the changes in $repository before you build a release." >&2
    exit 1
fi
echo "Verified the clean Kodosi source $(git -C "$repository" rev-parse HEAD)."
