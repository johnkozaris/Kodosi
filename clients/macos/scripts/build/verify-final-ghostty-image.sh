#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BINARY=${1:?Usage: verify-final-ghostty-image.sh <binary> <link-map> <deployment-target>}
LINK_MAP=${2:?Usage: verify-final-ghostty-image.sh <binary> <link-map> <deployment-target>}
MAX_DEPLOYMENT=${3:?Usage: verify-final-ghostty-image.sh <binary> <link-map> <deployment-target>}
LOCK_FILE="$ROOT/../../Ghostty.lock"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"

LIPO=${KODOSI_LIPO:-lipo}
SHASUM=${KODOSI_SHASUM:-shasum}
NM=${KODOSI_NM:-nm}
OTOOL=${KODOSI_OTOOL:-otool}
FIND=${KODOSI_FIND:-find}

combined_archive_sha256=

source "$LOCK_FILE"
if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
    source "$GHOSTTY_DIR/Vendor/libghostty.version"
fi
archive="$GHOSTTY_DIR/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
symbol_manifest="$GHOSTTY_DIR/Vendor/symbols/combined-strong.txt"

[[ -x "$BINARY" ]]
[[ -f "$LINK_MAP" ]]
[[ -f "$symbol_manifest" ]]
[[ "$($LIPO -archs "$BINARY")" == "$(uname -m)" || "$($LIPO -archs "$BINARY")" == arm64 ]]
[[ "$($SHASUM -a 256 "$archive" | awk '{print $1}')" == "$combined_archive_sha256" ]]

archive_paths=$(LC_ALL=C awk '
    /^\[[[:space:]]*[0-9]+\][[:space:]]+\/.*libghostty\.a\(/ {
        row=$0
        sub(/^\[[[:space:]]*[0-9]+\][[:space:]]+/, "", row)
        sub(/\(.*/, "", row)
        print row
    }
' "$LINK_MAP" | LC_ALL=C sort -u)
archive_count=$(grep -c . <<<"$archive_paths" || true)
if [[ "$archive_count" -ne 1 ]]; then
    echo "Final link map must contain exactly one Ghostty archive source; found $archive_count." >&2
    printf '%s\n' "$archive_paths" >&2
    exit 1
fi
linked_archive=$(printf '%s\n' "$archive_paths")
if [[ "$($SHASUM -a 256 "$linked_archive" | awk '{print $1}')" != "$combined_archive_sha256" ]]; then
    echo "Final link used a Ghostty archive that does not match the lock." >&2
    exit 1
fi

python3 "$ROOT/scripts/build/verify-ghostty-link-provenance.py" \
    "$LINK_MAP" \
    "$symbol_manifest" \
    "$linked_archive" \
    "$BINARY"

for symbol in \
    _ghostty_surface_new \
    _ghostty_surface_checkpoint_restore \
    _ghostty_surface_set_display_id \
    _ghostty_terminal_new \
    _ghostty_terminal_vt_write; do
    if ! grep -F "$symbol" "$LINK_MAP" >/dev/null; then
        echo "Final link map is missing required Ghostty symbol $symbol." >&2
        exit 1
    fi
done

if "$NM" -u "$BINARY" | grep -E '(^|[[:space:]])_ghostty_' >/dev/null; then
    echo "Final app has unresolved Ghostty imports." >&2
    exit 1
fi

app="$(dirname "$(dirname "$(dirname "$BINARY")")")"
if "$FIND" "$app" -type f \( -name 'libghostty*.dylib' -o -path '*Ghostty*.framework/*' \) -print -quit |
    grep -q .; then
    echo "Final app bundles a dynamic Ghostty image." >&2
    exit 1
fi

deployment_versions=$("$OTOOL" -l "$BINARY" | awk '
    $1 == "cmd" { build = ($2 == "LC_BUILD_VERSION"); legacy = ($2 == "LC_VERSION_MIN_MACOSX"); next }
    build && $1 == "minos" { print $2; build = 0; next }
    legacy && $1 == "version" { print $2; legacy = 0 }
')
[[ -n "$deployment_versions" ]]
if ! awk -v maximum="$MAX_DEPLOYMENT" '
    function code(v, p) { split(v, p, "."); return (p[1]+0)*1000000+(p[2]+0)*1000+(p[3]+0) }
    code($1) > code(maximum) { exit 1 }
' <<<"$deployment_versions"; then
    echo "$BINARY exceeds macOS $MAX_DEPLOYMENT" >&2
    exit 1
fi

echo "Verified one locked Ghostty renderer/VT image in $BINARY."
