#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_FILE="$ROOT/Ghostty.lock"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/terminal/ghostty"}"

# Git Bash may lack shasum, and SwiftPM's build-tool sandbox on macOS runs
# without the PATH entry for sha256sum.
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1"
    else
        shasum -a 256 "$1"
    fi | awk '{print $1}'
}

if [[ ! -f "$LOCK_FILE" ]]; then
    echo "Ghostty lock is missing: $LOCK_FILE" >&2
    exit 1
fi

upstream_commit=
combined_archive_sha256=
linux_upstream_commit=
linux_vt_archive_sha256=
windows_upstream_commit=
windows_x86_64_vt_archive_sha256=
windows_arm64_vt_archive_sha256=

source "$LOCK_FILE"

for value in \
    "$upstream_commit" \
    "$combined_archive_sha256" \
    "$linux_upstream_commit" \
    "$linux_vt_archive_sha256" \
    "$windows_upstream_commit"; do
    if [[ -z "$value" ]]; then
        echo "Ghostty lock is incomplete: $LOCK_FILE" >&2
        exit 1
    fi
done

"$GHOSTTY_DIR/Script/verify-platform-pins.py" >/dev/null

if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
    source "$GHOSTTY_DIR/Vendor/libghostty.version"
    upstream_commit=$ghostty_commit
    source "$GHOSTTY_DIR/Vendor/GhosttyVt/linux-x86_64.version"
    linux_upstream_commit=$ghostty_commit
    linux_vt_archive_sha256=$vt_archive_sha256
    for layout in windows-x86_64 windows-arm64; do
        [[ -f "$GHOSTTY_DIR/Vendor/GhosttyVt/$layout.version" ]] || continue
        source "$GHOSTTY_DIR/Vendor/GhosttyVt/$layout.version"
        windows_upstream_commit=$ghostty_commit
        case "$layout" in
            windows-x86_64) windows_x86_64_vt_archive_sha256=$vt_archive_sha256 ;;
            windows-arm64) windows_arm64_vt_archive_sha256=$vt_archive_sha256 ;;
        esac
    done
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

actual_windows_upstream=$(tr -d '[:space:]' <"$GHOSTTY_DIR/WindowsGhostty.ref")
if [[ "$actual_windows_upstream" != "$windows_upstream_commit" ]]; then
    echo "Windows Ghostty upstream revision mismatch: expected $windows_upstream_commit, found $actual_windows_upstream" >&2
    exit 1
fi
# Windows libraries are built on Windows hosts; each one vendored must match its pin.
for layout in windows-x86_64 windows-arm64; do
    windows_archive="$GHOSTTY_DIR/Vendor/GhosttyVt/$layout/lib/ghostty-vt-static.lib"
    [[ -e "$windows_archive" ]] || continue
    case "$layout" in
        windows-x86_64) expected=$windows_x86_64_vt_archive_sha256 ;;
        windows-arm64) expected=$windows_arm64_vt_archive_sha256 ;;
    esac
    actual=$(sha256 "$windows_archive")
    if [[ -z "$expected" || "$actual" != "$expected" ]]; then
        echo "$layout Ghostty VT archive mismatch: expected ${expected:-a pin in Ghostty.lock}, found $actual" >&2
        exit 1
    fi
done

renderer_archive="$GHOSTTY_DIR/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
vt_archive="$GHOSTTY_DIR/Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a"
linux_vt_archive="$GHOSTTY_DIR/Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
actual_archive_sha=$(sha256 "$renderer_archive")
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
actual_linux_archive_sha=$(sha256 "$linux_vt_archive")
if [[ "$actual_linux_archive_sha" != "$linux_vt_archive_sha256" ]]; then
    echo "Linux Ghostty VT archive mismatch: expected $linux_vt_archive_sha256, found $actual_linux_archive_sha" >&2
    exit 1
fi

echo "Verified Ghostty at macOS upstream $upstream_commit, Linux upstream $linux_upstream_commit and Windows upstream $windows_upstream_commit."
