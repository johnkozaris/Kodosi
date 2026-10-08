#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

case "${CONFIGURATION:-}" in
    Release)
        "$ROOT/scripts/build/rust-lib-release.sh"
        ;;
    WorkingTreeValidation)
        "$ROOT/scripts/build/rust-lib-release.sh" working-tree-validation
        ;;
    Debug)
        "$ROOT/scripts/build/rust-lib-debug.sh"
        ;;
    *)
        echo "Unsupported Xcode configuration for native preparation: ${CONFIGURATION:-<unset>}" >&2
        exit 1
        ;;
esac
