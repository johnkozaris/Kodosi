#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
cd "$ROOT"
"$ROOT/scripts/build/verify-kodosi-checkout.sh"
"$ROOT/scripts/build/xcode-gen.sh"
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
    echo "Project generation changed the release checkout: $ROOT" >&2
    exit 1
fi
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
: "${KODOSI_MARKETING_VERSION:?Set KODOSI_MARKETING_VERSION to the public semantic version}"
: "${KODOSI_BUILD_NUMBER:?Set KODOSI_BUILD_NUMBER to a monotonically increasing integer}"
rm -rf build/archive
xcodebuild -project KodosiDesktop.xcodeproj \
           -scheme KodosiDesktop \
           -configuration Release \
           -archivePath build/archive/KodosiDesktop.xcarchive \
           -destination 'generic/platform=macOS' \
           CURRENT_PROJECT_VERSION="$KODOSI_BUILD_NUMBER" \
           MARKETING_VERSION="$KODOSI_MARKETING_VERSION" \
           CODE_SIGN_STYLE=Manual \
           CODE_SIGN_IDENTITY="${KODOSI_SIGNING_IDENTITY:-Developer ID Application}" \
           DEVELOPMENT_TEAM="$KODOSI_TEAM_ID" \
           archive
"$ROOT/scripts/build/verify-native-notices.sh" \
    "$ROOT/build/archive/KodosiDesktop.xcarchive/Products/Applications/KodosiDesktop.app/Contents/Resources/GhosttyNativeNotices"
