#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_FILE="$ROOT/Ghostty.lock"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/terminal/ghostty"}"

if [[ ! -f "$LOCK_FILE" ]]; then
    echo "Ghostty lock is missing: $LOCK_FILE" >&2
    exit 1
fi

upstream_commit=
combined_archive_sha256=
ios_combined_archive_sha256=
ios_simulator_combined_archive_sha256=
linux_upstream_commit=
linux_vt_archive_sha256=

source "$LOCK_FILE"

for value in \
    "$upstream_commit" \
    "$combined_archive_sha256" \
    "$ios_combined_archive_sha256" \
    "$ios_simulator_combined_archive_sha256" \
    "$linux_upstream_commit" \
    "$linux_vt_archive_sha256"; do
    if [[ -z "$value" ]]; then
        echo "Ghostty lock is incomplete: $LOCK_FILE" >&2
        exit 1
    fi
done

"$GHOSTTY_DIR/Script/verify-platform-pins.py" >/dev/null

if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
    source "$GHOSTTY_DIR/Vendor/libghostty.version"
    upstream_commit=$ghostty_commit
    ios_combined_archive_sha256=$(sed -n 's/^combined_archive_sha256=//p' "$GHOSTTY_DIR/Vendor/libghostty-ios-arm64.version")
    ios_simulator_combined_archive_sha256=$(sed -n 's/^combined_archive_sha256=//p' "$GHOSTTY_DIR/Vendor/libghostty-ios-arm64-simulator.version")
    source "$GHOSTTY_DIR/Vendor/GhosttyVt/linux-x86_64.version"
    linux_upstream_commit=$ghostty_commit
    linux_vt_archive_sha256=$vt_archive_sha256
    printf "%s\n" "Validating non-publishable Ghostty working-tree artifacts." >&2
fi

actual_upstream=$(tr -d '[:space:]' <"$GHOSTTY_DIR/MacOSGhostty.ref")
if [[ "$actual_upstream" != "$upstream_commit" ]]; then
    echo "macOS Ghostty upstream revision mismatch: expected $upstream_commit, found $actual_upstream" >&2
    exit 1
fi
actual_linux_upstream=$(tr -d '[:space:]' <"$GHOSTTY_DIR/LinuxGhostty.ref")
if [[ "$actual_linux_upstream" != "$linux_upstream_commit" ]]; then
    echo "Linux Ghostty upstream revision mismatch: expected $linux_upstream_commit, found $actual_linux_upstream" >&2
    exit 1
fi

renderer_archive="$GHOSTTY_DIR/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
vt_archive="$GHOSTTY_DIR/Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a"
linux_vt_archive="$GHOSTTY_DIR/Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
actual_archive_sha=$(shasum -a 256 "$renderer_archive" | awk '{print $1}')
if [[ "$actual_archive_sha" != "$combined_archive_sha256" ]]; then
    echo "Combined Ghostty archive mismatch: expected $combined_archive_sha256, found $actual_archive_sha" >&2
    exit 1
fi
if [[ "$(uname -s)" == Darwin ]] &&
   [[ "$(lipo -archs "$renderer_archive")" != arm64 ]]; then
    echo "Ghostty renderer archive is not an arm64 artifact." >&2
    exit 1
fi
if ! cmp -s "$renderer_archive" "$vt_archive"; then
    echo "Ghostty renderer and VT archive payloads are not byte-identical arm64 artifacts." >&2
    exit 1
fi
for slice in ios-arm64 ios-arm64-simulator; do
    if [[ "$slice" == ios-arm64 ]]; then
        expected_ios_sha=$ios_combined_archive_sha256
    else
        expected_ios_sha=$ios_simulator_combined_archive_sha256
    fi
    actual_ios_sha=$(shasum -a 256 "$GHOSTTY_DIR/Vendor/GhosttyKit.xcframework/$slice/libghostty.a" | awk '{print $1}')
    if [[ "$actual_ios_sha" != "$expected_ios_sha" ]]; then
        echo "Ghostty $slice archive mismatch: expected $expected_ios_sha, found $actual_ios_sha" >&2
        exit 1
    fi
done
actual_linux_archive_sha=$(shasum -a 256 "$linux_vt_archive" | awk '{print $1}')
if [[ "$actual_linux_archive_sha" != "$linux_vt_archive_sha256" ]]; then
    echo "Linux Ghostty VT archive mismatch: expected $linux_vt_archive_sha256, found $actual_linux_archive_sha" >&2
    exit 1
fi

echo "Verified Ghostty at macOS and iOS upstream $upstream_commit and Linux upstream $linux_upstream_commit."
