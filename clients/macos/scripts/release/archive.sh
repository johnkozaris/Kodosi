#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$ROOT"
"$ROOT/scripts/build/verify-kodosi-checkout.sh"
"$ROOT/scripts/build/generate-xcode-project.sh"
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
    echo "Project generation changed the release checkout: $ROOT" >&2
    exit 1
fi
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
: "${KODOSI_MARKETING_VERSION:?Set KODOSI_MARKETING_VERSION to the public semantic version}"
: "${KODOSI_BUILD_NUMBER:?Set KODOSI_BUILD_NUMBER to a monotonically increasing integer}"
: "${KODOSI_PROVISIONING_PROFILE:?Set KODOSI_PROVISIONING_PROFILE to the name of the Developer ID provisioning profile}"
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
           KODOSI_PROVISIONING_PROFILE="$KODOSI_PROVISIONING_PROFILE" \
           DEVELOPMENT_TEAM="$KODOSI_TEAM_ID" \
           archive
