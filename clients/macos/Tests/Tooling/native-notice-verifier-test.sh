#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-native-notice-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
export KODOSI_GHOSTTY_DIR="$GHOSTTY_DIR"

expect_failure() {
    local label=$1
    shift
    if "$@" >/dev/null 2>&1; then
        echo "Expected native notice verification to fail: $label" >&2
        exit 1
    fi
}

fresh_payload() {
    rm -rf "$TMP/payload"
    "$ROOT/scripts/build/sync-native-notices.sh" "$TMP/payload"
}

fresh_payload
"$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload" >/dev/null

rm "$TMP/payload/LICENSE-GHOSTTY"
expect_failure missing-file "$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload"

fresh_payload
printf '\nchanged\n' >>"$TMP/payload/THIRD_PARTY_NOTICES.md"
expect_failure changed-file "$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload"

fresh_payload
printf 'extra\n' >"$TMP/payload/extra.txt"
expect_failure extra-file "$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload"

fresh_payload
rm "$TMP/payload/LICENSE"
ln -s "$GHOSTTY_DIR/LICENSE" "$TMP/payload/LICENSE"
expect_failure symlink-substitution "$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload"

fresh_payload
chmod -x "$TMP/payload/Script/relink-libintl.sh"
expect_failure nonexecutable-relink-helper "$ROOT/scripts/build/verify-native-notices.sh" "$TMP/payload"

echo "Native notice verifier mutation tests passed."
