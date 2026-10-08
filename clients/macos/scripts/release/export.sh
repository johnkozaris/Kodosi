#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
cd "$ROOT"
"$ROOT/scripts/release/archive.sh"
"$ROOT/scripts/release/verify-signed.sh"
"$ROOT/scripts/release/verify-publishable.sh" "$ROOT/build/archive/KodosiDesktop.xcarchive"
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
rm -rf build/export
export_options="build/ExportOptions.plist"
cp Config/ExportOptions.plist "$export_options"
/usr/libexec/PlistBuddy -c "Add :teamID string $KODOSI_TEAM_ID" "$export_options"
if [[ $(/usr/libexec/PlistBuddy -c 'Print :signingStyle' "$export_options") != manual ]] ||
   [[ $(/usr/libexec/PlistBuddy -c 'Print :signingCertificate' "$export_options") != 'Developer ID Application' ]]; then
    echo "Export must preserve manual Developer ID signing." >&2
    exit 1
fi
xcodebuild -exportArchive \
           -archivePath build/archive/KodosiDesktop.xcarchive \
           -exportOptionsPlist "$export_options" \
           -exportPath build/export
"$ROOT/scripts/release/verify-signed.sh" "$ROOT/build/export/KodosiDesktop.app"
"$ROOT/scripts/build/verify-native-notices.sh" \
    "$ROOT/build/export/KodosiDesktop.app/Contents/Resources/GhosttyNativeNotices"
cd build/export && /usr/bin/ditto -c -k --keepParent KodosiDesktop.app KodosiDesktop.zip
