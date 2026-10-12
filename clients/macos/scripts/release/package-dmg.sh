#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 || $2 != *.dmg ]]; then
    echo "usage: $0 <KodosiDesktop.app> <disk-image.dmg>" >&2
    exit 64
fi
app=$1
dmg=$2
: "${KODOSI_NOTARY_PROFILE:?Set KODOSI_NOTARY_PROFILE to the name of the notarytool keychain profile}"
if ! xcrun stapler validate "$app"; then
    echo "$app has no valid notarization ticket. Notarize the app with notarize.sh." >&2
    exit 1
fi
staging=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-dmg.XXXXXX")
trap 'rm -rf "$staging"' EXIT
/usr/bin/ditto "$app" "$staging/KodosiDesktop.app"
ln -s /Applications "$staging/Applications"
mkdir -p "$(dirname "$dmg")"
hdiutil create \
    -volname "KodosiDesktop" \
    -srcfolder "$staging" \
    -ov \
    -format UDZO \
    "$dmg"
codesign \
    --force \
    --sign "${KODOSI_SIGNING_IDENTITY:-Developer ID Application}" \
    --timestamp \
    "$dmg"
codesign --verify --strict --verbose=4 "$dmg"
xcrun notarytool submit "$dmg" \
    --keychain-profile "$KODOSI_NOTARY_PROFILE" \
    --wait
xcrun stapler staple "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=4 "$dmg"
