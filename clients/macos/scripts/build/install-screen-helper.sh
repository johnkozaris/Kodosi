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
signing=(--force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --options runtime "$timestamp")
if [[ $EXPANDED_CODE_SIGN_IDENTITY != - && -n ${DEVELOPMENT_TEAM:-} ]]; then
    # Kodosi Screen keeps the accounts that the person at the Mac allowed in a keychain group of
    # its own, which macOS lets it use only with a provisioning profile that grants the group.
    team=$DEVELOPMENT_TEAM
    application="$team.com.kodosi.screen"
    profiles="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
    fail() {
        echo "error: Kodosi Screen: $1" >&2
        exit 1
    }
    field() {
        /usr/bin/plutil -extract "$2" raw -o - - <<<"$1" 2>/dev/null
    }
    grants() {
        local granted expires
        granted=$(field "$1" 'Entitlements.com\.apple\.application-identifier') || return 1
        [[ $granted == "$application" || $granted == "$team.*" ]] || return 1
        granted=$(/usr/bin/plutil -extract Entitlements.keychain-access-groups json -o - - <<<"$1" 2>/dev/null) || return 1
        [[ $granted == *"\"$application\""* || $granted == *"\"$team.*\""* ]] || return 1
        expires=$(field "$1" ExpirationDate) || return 1
        expires=${expires//[^0-9]/}
        [[ ${#expires} -eq 14 ]] && ((10#$expires > 10#$(date -u +%Y%m%d%H%M%S)))
    }
    profile=
    if [[ -n ${KODOSI_SCREEN_PROVISIONING_PROFILE:-} ]]; then
        for candidate in "$profiles"/*.provisionprofile; do
            decoded=$(/usr/bin/security cms -D -i "$candidate" 2>/dev/null) || continue
            if [[ $(field "$decoded" Name) == "$KODOSI_SCREEN_PROVISIONING_PROFILE" ]] && grants "$decoded"; then
                profile=$candidate
                break
            fi
        done
        [[ -n $profile ]] ||
            fail "no valid provisioning profile \"$KODOSI_SCREEN_PROVISIONING_PROFILE\" for $application is installed in $profiles."
    else
        # Xcode chose the app's profile, which a team's development profile for all its apps also
        # grants to Kodosi Screen.
        candidate="$profiles/${EXPANDED_PROVISIONING_PROFILE:-none}.provisionprofile"
        decoded=$(/usr/bin/security cms -D -i "$candidate" 2>/dev/null) ||
            fail "Xcode chose no provisioning profile that is installed in $profiles."
        grants "$decoded" ||
            fail "the profile \"$(field "$decoded" Name)\" is not for $application with its keychain group. Set KODOSI_SCREEN_PROVISIONING_PROFILE to the name of a profile that is."
        profile=$candidate
    fi
    cp "$profile" "$app/Contents/embedded.provisionprofile"
    entitlements=$(mktemp "${TMPDIR:-/tmp}/kodosi-screen-entitlements.XXXXXX")
    trap 'rm -f "$entitlements"' EXIT
    cat >"$entitlements" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.application-identifier</key>
    <string>$application</string>
    <key>com.apple.developer.team-identifier</key>
    <string>$team</string>
    <key>keychain-access-groups</key>
    <array>
        <string>$application</string>
    </array>
</dict>
</plist>
EOF
    signing+=(--entitlements "$entitlements")
fi
/usr/bin/codesign "${signing[@]}" "$app"
