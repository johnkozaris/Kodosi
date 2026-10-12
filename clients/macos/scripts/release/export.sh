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
if [[ ! -d "$archive/Products/Applications/KodosiDesktop.app" ]]; then
    echo "$archive has no KodosiDesktop.app. Make the archive with archive.sh." >&2
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
rm -rf "$folder/KodosiDesktop.app"
xcodebuild -exportArchive \
           -archivePath "$archive" \
           -exportOptionsPlist "$options/ExportOptions.plist" \
           -exportPath "$folder"
