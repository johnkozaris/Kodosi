#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
: "${KODOSI_NOTARY_PROFILE:?Set KODOSI_NOTARY_PROFILE to the name of the notarytool keychain profile}"
"$ROOT/scripts/release/export.sh"
xcrun notarytool submit build/export/KodosiDesktop.zip \
    --keychain-profile "$KODOSI_NOTARY_PROFILE" \
    --wait
xcrun stapler staple build/export/KodosiDesktop.app
spctl --assess --type execute --verbose=4 build/export/KodosiDesktop.app
