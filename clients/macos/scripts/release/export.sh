#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <archive.xcarchive> <export-folder>" >&2
    exit 64
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
archive=$1
folder=$2
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
: "${KODOSI_PROVISIONING_PROFILE:?Set KODOSI_PROVISIONING_PROFILE to the name of the Developer ID provisioning profile}"
if [[ ! -d "$archive/Products/Applications/Kodosi.app" ]]; then
    echo "$archive has no Kodosi.app. Make the archive with archive.sh." >&2
    exit 1
fi
options=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-export.XXXXXX")
trap 'rm -rf "$options"' EXIT
cp "$ROOT/Config/ExportOptions.plist" "$options/ExportOptions.plist"
/usr/libexec/PlistBuddy -c "Add :teamID string $KODOSI_TEAM_ID" "$options/ExportOptions.plist"
/usr/libexec/PlistBuddy \
    -c "Add :provisioningProfiles dict" \
    -c "Add :provisioningProfiles:com.kodosi.desktop string $KODOSI_PROVISIONING_PROFILE" \
    "$options/ExportOptions.plist"
rm -rf "$folder/Kodosi.app"
xcodebuild -exportArchive \
           -archivePath "$archive" \
           -exportOptionsPlist "$options/ExportOptions.plist" \
           -exportPath "$folder"

# Xcode's export gives a nested app a provisioning profile only when the profile names that app's
# identifier. Kodosi Screen carries the profile of the app that contains it, so it gets the profile
# here, from the certificate that Xcode signed the app with, and the app's seal is made again over it.
app="$folder/Kodosi.app"
/usr/bin/codesign --display --extract-certificates="$options/certificate" "$app" 2>/dev/null
identity=$(/usr/bin/openssl x509 -inform DER -in "$options/certificate0" -noout -fingerprint -sha1 | sed 's/^.*=//; s/://g')
"$ROOT/scripts/build/sign-screen-helper.sh" "$app/Contents/Helpers/Kodosi Screen.app" "$identity" "$KODOSI_TEAM_ID" \
    "$app/Contents/embedded.provisionprofile" --timestamp
/usr/bin/codesign --display --entitlements "$options/entitlements.plist" --xml "$app" 2>/dev/null
/usr/bin/codesign --force --sign "$identity" --options runtime --timestamp --entitlements "$options/entitlements.plist" "$app"
