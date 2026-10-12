#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$ROOT"
"$ROOT/scripts/release/archive.sh"
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
: "${KODOSI_PROVISIONING_PROFILE:?Set KODOSI_PROVISIONING_PROFILE to the name of the Developer ID provisioning profile}"
rm -rf build/export
export_options="build/ExportOptions.plist"
cp Config/ExportOptions.plist "$export_options"
/usr/libexec/PlistBuddy -c "Add :teamID string $KODOSI_TEAM_ID" "$export_options"
/usr/libexec/PlistBuddy \
    -c "Add :provisioningProfiles dict" \
    -c "Add :provisioningProfiles:com.kodosi.desktop string $KODOSI_PROVISIONING_PROFILE" \
    "$export_options"
xcodebuild -exportArchive \
           -archivePath build/archive/KodosiDesktop.xcarchive \
           -exportOptionsPlist "$export_options" \
           -exportPath build/export
"$ROOT/scripts/release/verify-signed.sh" "$ROOT/build/export/KodosiDesktop.app"
cd build/export && /usr/bin/ditto -c -k --keepParent KodosiDesktop.app KodosiDesktop.zip
