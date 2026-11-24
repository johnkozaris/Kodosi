#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <resources-folder>" >&2
    exit 64
fi
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
python3 "$repository/scripts/build-document-views.py" --output-dir "$1/DocumentViews"
