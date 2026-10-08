#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"
DESTINATION="${1:?Pass the GhosttyNativeNotices directory to verify}"

"$ROOT/scripts/build/verify-ghostty-checkout.sh" >/dev/null
if [[ "$(uname -s)" == Darwin ]]; then
    "$GHOSTTY_DIR/Script/verify-third-party-notices.py" >/dev/null
else
    "$GHOSTTY_DIR/Script/verify-linux-third-party-notices.py" >/dev/null
fi

if [[ ! -d "$DESTINATION" ]] || [[ -L "$DESTINATION" ]]; then
    echo "Native notice directory is missing or unsafe: $DESTINATION" >&2
    exit 1
fi

expected=$(mktemp "${TMPDIR:-/tmp}/kodosi-native-notices.XXXXXX")
actual=$(mktemp "${TMPDIR:-/tmp}/kodosi-native-notices.XXXXXX")
trap 'rm -f "$expected" "$actual"' EXIT

{
    printf '%s\n' LICENSE LICENSE-GHOSTTY THIRD_PARTY_NOTICES.md
    printf '%s\n' Script/relink-libintl.sh
    printf '%s\n' Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a
    find "$GHOSTTY_DIR/ThirdPartyNotices" -type f -print | while IFS= read -r path; do
        printf 'ThirdPartyNotices/%s\n' "${path#"$GHOSTTY_DIR/ThirdPartyNotices/"}"
    done
} | LC_ALL=C sort >"$expected"

if find "$DESTINATION" -type l -print -quit | grep -q .; then
    echo "Native notice payload contains a symlink: $DESTINATION" >&2
    exit 1
fi
if find "$DESTINATION" ! -type d ! -type f -print -quit | grep -q .; then
    echo "Native notice payload contains a non-regular entry: $DESTINATION" >&2
    exit 1
fi
find "$DESTINATION" -type f -print | while IFS= read -r path; do
    printf '%s\n' "${path#"$DESTINATION/"}"
done | LC_ALL=C sort >"$actual"

if ! cmp -s "$expected" "$actual"; then
    echo "Native notice payload path set differs from the pinned Ghostty contract." >&2
    diff -u "$expected" "$actual" >&2 || true
    exit 1
fi

while IFS= read -r relative; do
    if ! cmp -s "$GHOSTTY_DIR/$relative" "$DESTINATION/$relative"; then
        echo "Native notice payload differs from pinned Ghostty file: $relative" >&2
        exit 1
    fi
done <"$expected"

if [[ ! -x "$DESTINATION/Script/relink-libintl.sh" ]]; then
    echo "Bundled libintl relink helper is not executable." >&2
    exit 1
fi

echo "Verified exact native notice payload from pinned Ghostty checkout."
