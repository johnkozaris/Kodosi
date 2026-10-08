#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
"$ROOT/scripts/release/export.sh"
(
    xcrun notarytool submit build/export/KodosiDesktop.zip \
        --keychain-profile kodosi-notary \
        --wait
)
(
    xcrun stapler staple build/export/KodosiDesktop.app
)
(
    xcrun stapler validate build/export/KodosiDesktop.app
)
(
    spctl --assess --type execute --verbose=4 build/export/KodosiDesktop.app
)
