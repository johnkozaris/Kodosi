#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFIER="$ROOT/scripts/release/verify-provisioning-profile.sh"
SCRATCH="$ROOT/build/test-provisioning-profile"
FAKE_SECURITY="$SCRATCH/security"
FAKE_CODESIGN="$SCRATCH/codesign"
APP="$SCRATCH/KodosiDesktop.app"
TEAM_ID=ABCDE12345

cleanup() {
    rm -rf "$SCRATCH"
}
trap cleanup EXIT
cleanup
mkdir -p "$APP/Contents"

cat >"$FAKE_SECURITY" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1 $2 $3" == "cms -D -i" && -f "$4" ]] || exit 1
printf '%s\n' "${FAKE_PROFILE:?}"
EOF
cat >"$FAKE_CODESIGN" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
    case "$argument" in
        --entitlements) printf '%s\n' "${FAKE_ENTITLEMENTS:?}"; exit 0 ;;
        --extract-certificates=*) cp "${FAKE_LEAF:?}" "${argument#*=}0"; exit 0 ;;
    esac
done
exit 1
EOF
chmod +x "$FAKE_SECURITY" "$FAKE_CODESIGN"

/usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string com.kodosi.desktop" "$APP/Contents/Info.plist" >/dev/null
printf 'profile' >"$APP/Contents/embedded.provisionprofile"
printf 'leaf certificate' >"$SCRATCH/leaf.der"
printf 'other certificate' >"$SCRATCH/other.der"
leaf=$(base64 -i "$SCRATCH/leaf.der")
other=$(base64 -i "$SCRATCH/other.der")

good_profile="<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
    <key>DeveloperCertificates</key>
    <array><data>$other</data><data>$leaf</data></array>
    <key>Entitlements</key>
    <dict>
        <key>com.apple.application-identifier</key><string>ABCDE12345.com.kodosi.desktop</string>
        <key>com.apple.developer.team-identifier</key><string>ABCDE12345</string>
        <key>keychain-access-groups</key><array><string>ABCDE12345.*</string></array>
    </dict>
    <key>ExpirationDate</key><date>2044-10-11T12:00:00Z</date>
    <key>Platform</key><array><string>OSX</string></array>
    <key>ProvisionsAllDevices</key><true/>
    <key>TeamIdentifier</key><array><string>ABCDE12345</string></array>
</dict>
</plist>"

good_entitlements='<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.application-identifier</key><string>ABCDE12345.com.kodosi.desktop</string>
    <key>com.apple.developer.team-identifier</key><string>ABCDE12345</string>
    <key>keychain-access-groups</key><array><string>ABCDE12345.com.kodosi.desktop</string></array>
</dict>
</plist>'

verify() {
    FAKE_PROFILE=$1 FAKE_ENTITLEMENTS=$2 FAKE_LEAF="$SCRATCH/leaf.der" \
        "$VERIFIER" "$APP" "${3:-$TEAM_ID}" "$FAKE_SECURITY" "$FAKE_CODESIGN"
}

assert_rejected() {
    local name=$1 expected=$2
    if verify "$3" "$4" "${5:-$TEAM_ID}" >"$SCRATCH/$name.log" 2>&1; then
        echo "Provisioning profile verifier accepted $name." >&2
        exit 1
    fi
    grep -F -q "$expected" "$SCRATCH/$name.log" || {
        echo "Provisioning profile verifier rejected $name for another reason:" >&2
        cat "$SCRATCH/$name.log" >&2
        exit 1
    }
}

verify "$good_profile" "$good_entitlements"
verify "${good_profile/ABCDE12345.\*/ABCDE12345.com.kodosi.desktop}" "$good_entitlements" >/dev/null

assert_rejected development-profile "not a Developer ID profile" \
    "${good_profile/<key>ProvisionsAllDevices<\/key><true\/>/<key>ProvisionedDevices</key><array/>}" "$good_entitlements"
assert_rejected ios-profile "not for macOS" \
    "${good_profile/<string>OSX<\/string>/<string>iOS</string>}" "$good_entitlements"
assert_rejected wrong-team "not for team" \
    "${good_profile//<string>ABCDE12345<\/string>/<string>AAAAAAAAAA</string>}" "$good_entitlements"
assert_rejected wildcard-app "not for ABCDE12345.com.kodosi.desktop" \
    "${good_profile/ABCDE12345.com.kodosi.desktop/ABCDE12345.*}" "$good_entitlements"
assert_rejected expired "has expired" \
    "${good_profile/2044-10-11T12:00:00Z/2020-10-11T12:00:00Z}" "$good_entitlements"
assert_rejected no-keychain-group "does not grant the Keychain access group" \
    "${good_profile/ABCDE12345.\*/ABCDE12345.com.kodosi.other}" "$good_entitlements"
assert_rejected unlisted-certificate "does not list the signing certificate" \
    "${good_profile/<data>$leaf<\/data>/}" "$good_entitlements"
assert_rejected extra-group "exactly the Keychain access group" \
    "$good_profile" "${good_entitlements/<\/string><\/array>/</string><string>ABCDE12345.shared</string></array>}"
assert_rejected other-group "exactly the Keychain access group" \
    "$good_profile" "${good_entitlements/<array><string>ABCDE12345.com.kodosi.desktop/<array><string>ABCDE12345.com.kodosi.other}"
assert_rejected no-application-identifier "does not name the application identifier" \
    "$good_profile" "${good_entitlements/com.apple.application-identifier/com.example.other}"
assert_rejected no-team-identifier "does not name team" \
    "$good_profile" "${good_entitlements/com.apple.developer.team-identifier/com.example.other}"
assert_rejected debuggable "allows debugging" \
    "$good_profile" "${good_entitlements/<dict>/<dict><key>com.apple.security.get-task-allow</key><true/>}"
assert_rejected team-format "10-character Apple team identifier" "$good_profile" "$good_entitlements" invalid

rm "$APP/Contents/embedded.provisionprofile"
assert_rejected no-profile "no embedded provisioning profile" "$good_profile" "$good_entitlements"

echo "Verified Developer ID provisioning profile enforcement."
