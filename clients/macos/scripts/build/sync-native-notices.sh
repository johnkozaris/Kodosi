#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"
DESTINATION="${1:?Pass the destination notice directory}"

parent=$(dirname "$DESTINATION")
mkdir -p "$parent"
staging=$(mktemp -d "$parent/.GhosttyNativeNotices.XXXXXX")
trap 'rm -rf "$staging"' EXIT

cp "$GHOSTTY_DIR/LICENSE" "$staging/LICENSE"
cp "$GHOSTTY_DIR/LICENSE-GHOSTTY" "$staging/LICENSE-GHOSTTY"
cp "$GHOSTTY_DIR/THIRD_PARTY_NOTICES.md" "$staging/THIRD_PARTY_NOTICES.md"
mkdir -p "$staging/Script" "$staging/Vendor/GhosttyKit.xcframework/macos-arm64"
cp "$GHOSTTY_DIR/Script/relink-libintl.sh" "$staging/Script/relink-libintl.sh"
cp "$GHOSTTY_DIR/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a" \
    "$staging/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
cp -R "$GHOSTTY_DIR/ThirdPartyNotices" "$staging/ThirdPartyNotices"

"$ROOT/scripts/build/verify-native-notices.sh" "$staging" >/dev/null
rm -rf "$DESTINATION"
mv "$staging" "$DESTINATION"
trap - EXIT
