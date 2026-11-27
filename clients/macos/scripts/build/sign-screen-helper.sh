#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
    echo "usage: $0 <Kodosi Screen.app> <signing-identity> <team-id> <provisioning-profile> <codesign-timestamp-option>" >&2
    exit 64
fi
app=$1
identity=$2
team=$3
profile=$4
timestamp=$5
group="$team.com.kodosi.screen"

fail() {
    echo "error: Kodosi Screen: $1" >&2
    exit 1
}

# Kodosi Screen keeps the accounts that the person at the Mac allowed in a keychain group of its
# own. macOS lets it use the group only with a provisioning profile that grants the group, and it
# carries the profile of the app that contains it.
decoded=$(/usr/bin/security cms -D -i "$profile" 2>/dev/null) ||
    fail "the provisioning profile $profile cannot be read."
granted=$(/usr/bin/plutil -extract Entitlements.keychain-access-groups json -o - - <<<"$decoded" 2>/dev/null) ||
    fail "the provisioning profile $profile grants no keychain group."
[[ $granted == *"\"$group\""* || $granted == *"\"$team.*\""* ]] ||
    fail "the provisioning profile $profile does not grant the keychain group $group."
expires=$(/usr/bin/plutil -extract ExpirationDate raw -o - - <<<"$decoded" 2>/dev/null) ||
    fail "the provisioning profile $profile has no end date."
expires=${expires//[^0-9]/}
[[ ${#expires} -eq 14 ]] && ((10#$expires > 10#$(date -u +%Y%m%d%H%M%S))) ||
    fail "the provisioning profile $profile has expired."

cp "$profile" "$app/Contents/embedded.provisionprofile"
entitlements=$(mktemp "${TMPDIR:-/tmp}/kodosi-screen-entitlements.XXXXXX")
trap 'rm -f "$entitlements"' EXIT
cat >"$entitlements" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.team-identifier</key>
    <string>$team</string>
    <key>keychain-access-groups</key>
    <array>
        <string>$group</string>
    </array>
</dict>
</plist>
EOF
/usr/bin/codesign --force --sign "$identity" --options runtime "$timestamp" --entitlements "$entitlements" "$app"
