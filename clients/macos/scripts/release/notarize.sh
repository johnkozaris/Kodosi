#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <KodosiDesktop.app>" >&2
    exit 64
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
: "${KODOSI_NOTARY_PROFILE:?Set KODOSI_NOTARY_PROFILE to the name of the notarytool keychain profile}"
if [[ ! -d "$1/Contents" ]]; then
    echo "$1 is not an app. Export the app with export.sh." >&2
    exit 1
fi
app=$(cd "$1" && pwd)
"$ROOT/scripts/release/verify-signed.sh" "$app"
upload=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-notarize.XXXXXX")
trap 'rm -rf "$upload"' EXIT
/usr/bin/ditto -c -k --keepParent "$app" "$upload/KodosiDesktop.zip"
xcrun notarytool submit "$upload/KodosiDesktop.zip" \
    --keychain-profile "$KODOSI_NOTARY_PROFILE" \
    --wait
xcrun stapler staple "$app"
spctl --assess --type execute --verbose=4 "$app"
