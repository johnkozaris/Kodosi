#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <Debug|Release|WorkingTreeValidation> <contents-folder> <resources-folder>" >&2
    exit 64
fi
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
# Xcode build phases run with a minimal PATH that excludes Cargo/Rust tools.
export PATH="$HOME/.cargo/bin:$PATH"
options=()
if [[ $1 == Debug ]]; then
    options=(--debug)
else
    options=(--notices "$3/ScreenNotices")
    export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"
fi
app=$(python3 "$repository/screen/build/build-helper.py" \
    --target aarch64-apple-darwin \
    --output-dir "$2/Helpers" \
    --icon "$3/AppIcon.icns" \
    "${options[@]}")
if [[ ${CODE_SIGNING_ALLOWED:-YES} != YES || -z ${EXPANDED_CODE_SIGN_IDENTITY:-} ]]; then
    exit 0
fi
timestamp=--timestamp=none
if [[ $1 != Debug ]]; then
    timestamp=--timestamp
fi
# Xcode's export of an archive gives a nested app a provisioning profile only when the profile names
# that app's identifier, so the release's export.sh gives Kodosi Screen its profile after the export.
if [[ $EXPANDED_CODE_SIGN_IDENTITY == - || -z ${DEVELOPMENT_TEAM:-} || ${ACTION:-} == install ]]; then
    /usr/bin/codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --options runtime "$timestamp" "$app"
    exit 0
fi
"$(dirname "${BASH_SOURCE[0]}")/sign-screen-helper.sh" "$app" "$EXPANDED_CODE_SIGN_IDENTITY" "$DEVELOPMENT_TEAM" \
    "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/${EXPANDED_PROVISIONING_PROFILE:-none}.provisionprofile" \
    "$timestamp"
