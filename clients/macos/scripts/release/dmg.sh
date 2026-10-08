#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
"$ROOT/scripts/release/notarize.sh"
staging="build/dmg-root"
dmg="build/export/KodosiDesktop.dmg"
rm -rf "$staging" "$dmg"
mkdir -p "$staging"
/usr/bin/ditto build/export/KodosiDesktop.app "$staging/KodosiDesktop.app"
"$ROOT/scripts/build/verify-native-notices.sh" \
    "$ROOT/$staging/KodosiDesktop.app/Contents/Resources/GhosttyNativeNotices"
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
    --keychain-profile kodosi-notary \
    --wait
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=4 "$dmg"
