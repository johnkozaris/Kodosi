#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || $1 != *.xcarchive ]]; then
    echo "usage: $0 <archive.xcarchive>" >&2
    exit 64
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
: "${KODOSI_PROVISIONING_PROFILE:?Set KODOSI_PROVISIONING_PROFILE to the name of the Developer ID provisioning profile}"
"$ROOT/../apple/scripts/verify-kodosi-checkout.sh"
rm -rf "$1"
xcodebuild -project "$ROOT/KodosiDesktop.xcodeproj" \
           -scheme KodosiDesktop \
           -configuration Release \
           -archivePath "$1" \
           -destination 'generic/platform=macOS' \
           CODE_SIGN_STYLE=Manual \
           CODE_SIGN_IDENTITY="${KODOSI_SIGNING_IDENTITY:-Developer ID Application}" \
           KODOSI_PROVISIONING_PROFILE="$KODOSI_PROVISIONING_PROFILE" \
           DEVELOPMENT_TEAM="$KODOSI_TEAM_ID" \
           archive
