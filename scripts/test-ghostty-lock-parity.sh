#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SWIFT_ROOT="${KODOSI_SWIFT_DIR:-$ROOT/../kodosiSwift}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/Kodosi/scripts" "$TMP/kodosiSwift/scripts/build"
cp "$ROOT/Ghostty.lock" "$TMP/Kodosi/Ghostty.lock"
cp "$SWIFT_ROOT/Ghostty.lock" "$TMP/kodosiSwift/Ghostty.lock"
cp "$ROOT/scripts/verify-ghostty-lock-parity.sh" "$TMP/Kodosi/scripts/"
cp "$SWIFT_ROOT/scripts/build/verify-ghostty-lock-parity.sh" "$TMP/kodosiSwift/scripts/build/"

expect_failure() {
    local description="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo "$description" >&2
        exit 1
    fi
}

expect_failure \
    "Kodosi verifier accepted a missing sibling checkout" \
    env KODOSI_SWIFT_DIR="$TMP/missing-kodosiSwift" \
    bash "$TMP/Kodosi/scripts/verify-ghostty-lock-parity.sh"

mkdir -p "$TMP/missing-lock-kodosiSwift"
expect_failure \
    "Kodosi verifier accepted a sibling checkout without Ghostty.lock" \
    env KODOSI_SWIFT_DIR="$TMP/missing-lock-kodosiSwift" \
    bash "$TMP/Kodosi/scripts/verify-ghostty-lock-parity.sh"

KODOSI_SWIFT_DIR="$TMP/kodosiSwift" bash "$TMP/Kodosi/scripts/verify-ghostty-lock-parity.sh"
KODOSI_DIR="$TMP/Kodosi" bash "$TMP/kodosiSwift/scripts/build/verify-ghostty-lock-parity.sh"
printf '\ndrift=true\n' >> "$TMP/kodosiSwift/Ghostty.lock"
expect_failure \
    "Kodosi verifier accepted a drifted sibling lock" \
    env KODOSI_SWIFT_DIR="$TMP/kodosiSwift" \
    bash "$TMP/Kodosi/scripts/verify-ghostty-lock-parity.sh"
expect_failure \
    "Swift verifier accepted a drifted sibling lock" \
    env KODOSI_DIR="$TMP/Kodosi" \
    bash "$TMP/kodosiSwift/scripts/build/verify-ghostty-lock-parity.sh"
