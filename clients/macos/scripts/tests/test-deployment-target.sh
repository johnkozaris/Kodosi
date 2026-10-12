#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-deployment-target-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

valid="$TMP/valid"
printf '27.1\n' >"$valid"
KODOSI_MACOS_DEPLOYMENT_TARGET_FILE="$valid" kodosi_load_macos_deployment_target "$ROOT"
test "$KODOSI_MACOS_DEPLOYMENT_TARGET" = 27.1
test "$MACOSX_DEPLOYMENT_TARGET" = 27.1

assert_rejected() {
    local name=$1
    local value=$2
    local authority="$TMP/$name"
    printf '%b' "$value" >"$authority"
    if (KODOSI_MACOS_DEPLOYMENT_TARGET_FILE="$authority" kodosi_load_macos_deployment_target "$ROOT") >/dev/null 2>&1; then
        echo "Accepted invalid macOS deployment target fixture: $name" >&2
        exit 1
    fi
}

assert_rejected empty ''
assert_rejected major-only '26\n'
assert_rejected text '26.x\n'
assert_rejected multiline '26.4\n27.0\n'
if (KODOSI_MACOS_DEPLOYMENT_TARGET_FILE="$TMP/missing" kodosi_load_macos_deployment_target "$ROOT") >/dev/null 2>&1; then
    echo "Accepted a missing macOS deployment target authority" >&2
    exit 1
fi

unset KODOSI_MACOS_DEPLOYMENT_TARGET_FILE
kodosi_load_macos_deployment_target "$ROOT"
printf 'Verified canonical macOS deployment-target authority and rejection cases.\n'
