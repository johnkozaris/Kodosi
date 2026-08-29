#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIBLING="${KODOSI_SWIFT_DIR:-$ROOT/../kodosiSwift}"
LOCAL="$ROOT/Ghostty.lock"
REMOTE="$SIBLING/Ghostty.lock"

if [[ ! -f "$LOCAL" ]]; then
    echo "Ghostty lock is missing: $LOCAL" >&2
    exit 1
fi
if [[ ! -d "$SIBLING" ]]; then
    echo "Required sibling Swift checkout is missing: $SIBLING" >&2
    exit 1
fi
if [[ ! -f "$REMOTE" ]]; then
    echo "Sibling Swift checkout exists but its Ghostty lock is missing: $REMOTE" >&2
    exit 1
fi
if ! cmp -s "$LOCAL" "$REMOTE"; then
    echo "Ghostty lock drifted between Kodosi and kodosiSwift" >&2
    diff -u "$LOCAL" "$REMOTE" >&2 || true
    exit 1
fi
