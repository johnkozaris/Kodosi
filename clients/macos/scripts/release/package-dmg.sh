#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
: "${KODOSI_NOTARY_PROFILE:?Set KODOSI_NOTARY_PROFILE to the name of the notarytool keychain profile}"
"$ROOT/scripts/release/notarize.sh"
staging="build/dmg-root"
dmg="build/export/KodosiDesktop.dmg"
rm -rf "$staging" "$dmg"
mkdir -p "$staging"
/usr/bin/ditto build/export/KodosiDesktop.app "$staging/KodosiDesktop.app"
ln -s /Applications "$staging/Applications"
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
