#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 4 ]]; then
    echo "usage: $0 <app> <expected-team-id> [security-command] [codesign-command]" >&2
    exit 2
fi

app=$1
expected_team_id=$2
security_command=${3:-/usr/bin/security}
codesign_command=${4:-/usr/bin/codesign}

if [[ ! "$expected_team_id" =~ ^[A-Z0-9]{10}$ ]]; then
    echo "Expected team ID must be a 10-character Apple team identifier." >&2
    exit 1
fi

fail() {
    echo "$app: $1" >&2
    exit 1
}

profile="$app/Contents/embedded.provisionprofile"
[[ -f "$profile" ]] || fail "no embedded provisioning profile."
bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")
application_id="$expected_team_id.$bundle_id"

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
decoded="$scratch/profile.plist"
signed="$scratch/entitlements.plist"
"$security_command" cms -D -i "$profile" >"$decoded" ||
    fail "the embedded provisioning profile cannot be decoded."
"$codesign_command" --display --entitlements - --xml "$app" >"$signed" 2>/dev/null ||
    fail "codesign cannot read the signed entitlements."
"$codesign_command" --display --extract-certificates="$scratch/certificate" "$app" 2>/dev/null ||
    fail "codesign cannot read the signing certificates."

extract() {
    plutil -extract "$2" "$3" -o - "$1" 2>/dev/null
}
entry() {
    /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null
}

[[ $(extract "$decoded" ProvisionsAllDevices raw) == true ]] ||
    fail "the embedded profile is not a Developer ID profile."
[[ $(extract "$decoded" Platform json) == *'"OSX"'* ]] ||
    fail "the embedded profile is not for macOS."
[[ $(extract "$decoded" TeamIdentifier.0 raw) == "$expected_team_id" ]] ||
    fail "the embedded profile is not for team $expected_team_id."
[[ $(entry "$decoded" Entitlements:com.apple.application-identifier) == "$application_id" ]] ||
    fail "the embedded profile is not for $application_id."

expires=$(extract "$decoded" ExpirationDate raw)
expires_digits=${expires//[^0-9]/}
now_digits=$(date -u +%Y%m%d%H%M%S)
if [[ ${#expires_digits} -ne 14 ]] || ((10#$expires_digits <= 10#$now_digits)); then
    fail "the embedded profile has expired."
fi

groups=$(extract "$decoded" Entitlements.keychain-access-groups json)
[[ $groups == *"\"$expected_team_id.*\""* || $groups == *"\"$application_id\""* ]] ||
    fail "the embedded profile does not grant the Keychain access group $application_id."

leaf="$scratch/certificate0"
[[ -s "$leaf" ]] || fail "the signing certificate cannot be read."
leaf_base64=$(base64 -i "$leaf")
listed=false
index=0
while certificate=$(extract "$decoded" "DeveloperCertificates.$index" raw); do
    [[ "$certificate" == "$leaf_base64" ]] && listed=true
    index=$((index + 1))
done
[[ $listed == true ]] || fail "the embedded profile does not list the signing certificate."

[[ $(extract "$signed" keychain-access-groups json) == "[\"$application_id\"]" ]] ||
    fail "the signature must hold exactly the Keychain access group $application_id."
[[ $(entry "$signed" com.apple.application-identifier) == "$application_id" ]] ||
    fail "the signature does not name the application identifier $application_id."
[[ $(entry "$signed" com.apple.developer.team-identifier) == "$expected_team_id" ]] ||
    fail "the signature does not name team $expected_team_id."
[[ $(entry "$signed" com.apple.security.get-task-allow) != true ]] ||
    fail "the signature allows debugging."

echo "Verified the Developer ID provisioning profile of $app (expires $expires)."
