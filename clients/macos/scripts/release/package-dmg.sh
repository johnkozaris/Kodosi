#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 || $2 != *.dmg ]]; then
    echo "usage: $0 <Kodosi.app> <disk-image.dmg>" >&2
    exit 64
fi
app=$1
dmg=$2
: "${KODOSI_NOTARY_PROFILE:?Set KODOSI_NOTARY_PROFILE to the name of the notarytool keychain profile}"
if ! xcrun stapler validate "$app"; then
    echo "$app has no valid notarization ticket. Notarize the app with notarize.sh." >&2
    exit 1
fi
release=$(dirname "$0")
tools="$release/../../.tools/dmg"
if ! cmp -s "$release/dmg-requirements.txt" "$tools/requirements.txt"; then
    uv venv --clear --quiet --python '>=3.12' "$tools"
    uv pip install --quiet --require-hashes --python "$tools/bin/python" -r "$release/dmg-requirements.txt"
    cp "$release/dmg-requirements.txt" "$tools/requirements.txt"
fi
"$tools/bin/python" "$release/make-dmg.py" "$app" "$dmg"
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
