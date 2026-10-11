#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"
BUNDLE="${1:?Pass the app bundle resources directory}"

notices="$BUNDLE/Notices"
source_dir="$BUNDLE/GhosttySource"
staging=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-notices.XXXXXX")
trap 'rm -rf "$staging"' EXIT

mkdir -p "$staging/Notices/Ghostty/Licenses" "$staging/GhosttySource"
cp "$ROOT/../apple/Notices/"*.txt "$staging/Notices/"
cp "$GHOSTTY_DIR/LICENSE" "$staging/Notices/Ghostty/libghostty-spm.txt"
cp "$GHOSTTY_DIR/LICENSE-GHOSTTY" "$staging/Notices/Ghostty/Ghostty.txt"
cp "$GHOSTTY_DIR/THIRD_PARTY_NOTICES.md" "$staging/Notices/Ghostty/Third-party notices.md"
cp "$GHOSTTY_DIR/ThirdPartyNotices/licenses/"*.txt "$staging/Notices/Ghostty/Licenses/"
cp "$GHOSTTY_DIR/ThirdPartyNotices/inventory.json" "$staging/GhosttySource/"
cp "$GHOSTTY_DIR/ThirdPartyNotices/corresponding-source/"*.tar* "$staging/GhosttySource/"

rm -rf "$notices" "$source_dir"
mv "$staging/Notices" "$notices"
mv "$staging/GhosttySource" "$source_dir"
