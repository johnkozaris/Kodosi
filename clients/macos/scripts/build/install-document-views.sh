#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <resources-folder>" >&2
    exit 64
fi
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
# Xcode gives its build phases a PATH without the Bun tools.
export PATH="$HOME/.bun/bin:/opt/homebrew/bin:$PATH"
"$repository/scripts/build-document-views.sh"
mkdir -p "$1"
rsync -a --delete "$repository/documents/views/dist/" "$1/DocumentViews/"
