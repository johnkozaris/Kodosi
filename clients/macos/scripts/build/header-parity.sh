#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

bundle=${1:?Usage: header-parity.sh <ffi-artifact-bundle>}
generated="$bundle/include/kodosi_runtime.h"
committed="$ROOT/Frameworks/KodosiKit.xcframework/Headers/kodosi_runtime.h"
if ! cmp -s "$generated" "$committed"; then
    diff -u "$committed" "$generated" >&2 || true
    echo "Committed kodosi_runtime.h differs from the selected artifact's cbindgen output." >&2
    echo "Regenerate the committed contract from the artifact header; do not hand-edit it." >&2
    exit 1
fi
