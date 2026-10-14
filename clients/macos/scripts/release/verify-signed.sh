#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$ROOT"

: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"

app=${1:-build/archive/KodosiDesktop.xcarchive/Products/Applications/KodosiDesktop.app}
"$ROOT/scripts/release/verify-publishable.sh" "$app"

/usr/bin/codesign --verify --deep --strict --verbose=4 "$app"
"$ROOT/scripts/release/verify-developer-id-signature.sh" "$app" "$KODOSI_TEAM_ID"
"$ROOT/scripts/release/verify-provisioning-profile.sh" "$app" "$KODOSI_TEAM_ID"
/usr/bin/codesign --display --verbose=4 "$app" 2>&1
