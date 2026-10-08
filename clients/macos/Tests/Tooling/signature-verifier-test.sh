#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFIER="$ROOT/scripts/release/verify-developer-id-signature.sh"
SCRATCH="$ROOT/build/signature-verifier-test"
FAKE_CODESIGN="$SCRATCH/codesign"
TEAM_ID=C5886CWK32

cleanup() {
    rm -rf "$SCRATCH"
}
trap cleanup EXIT
cleanup
mkdir -p "$SCRATCH"

cat >"$FAKE_CODESIGN" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *" --verify "* ]]; then
    [[ " $* " == *" -R=anchor apple generic and certificate leaf[subject.OU] = \"C5886CWK32\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists "* ]] || {
        echo "missing Developer ID designated requirement" >&2
        exit 1
    }
    exit "${FAKE_VERIFY_EXIT:-0}"
fi
printf '%s\n' "${FAKE_CODESIGN_DETAILS:?}"
EOF
chmod +x "$FAKE_CODESIGN"

good_details='Executable=/tmp/KodosiDesktop
Authority=Developer ID Application: Ioannis Kozaris (C5886CWK32)
Authority=Developer ID Certification Authority
Authority=Apple Root CA
Timestamp=Aug 14, 2026 at 12:00:00
TeamIdentifier=C5886CWK32
CodeDirectory v=20500 size=123 flags=0x10000(runtime) hashes=1+7 location=embedded'

assert_rejected() {
    local name=$1
    local details=$2
    if FAKE_CODESIGN_DETAILS="$details" \
        "$VERIFIER" /tmp/KodosiDesktop "$TEAM_ID" "$FAKE_CODESIGN" >"$SCRATCH/$name.log" 2>&1; then
        echo "Signature verifier accepted $name metadata." >&2
        exit 1
    fi
}

FAKE_CODESIGN_DETAILS="$good_details" \
    "$VERIFIER" /tmp/KodosiDesktop "$TEAM_ID" "$FAKE_CODESIGN"

assert_rejected adhoc "${good_details/Authority=Developer ID Application: Ioannis Kozaris (C5886CWK32)/Signature=adhoc}"
assert_rejected wrong-team "${good_details/TeamIdentifier=C5886CWK32/TeamIdentifier=AAAAAAAAAA}"
assert_rejected no-timestamp "${good_details/Timestamp=Aug 14, 2026 at 12:00:00/}"
assert_rejected no-runtime "${good_details/flags=0x10000(runtime)/flags=0x0(none)}"

if FAKE_CODESIGN_DETAILS="$good_details" FAKE_VERIFY_EXIT=1 \
    "$VERIFIER" /tmp/KodosiDesktop "$TEAM_ID" "$FAKE_CODESIGN" >"$SCRATCH/integrity.log" 2>&1; then
    echo "Signature verifier ignored codesign integrity failure." >&2
    exit 1
fi

if FAKE_CODESIGN_DETAILS="$good_details" \
    "$VERIFIER" /tmp/KodosiDesktop invalid "$FAKE_CODESIGN" >"$SCRATCH/team-format.log" 2>&1; then
    echo "Signature verifier accepted malformed team identifier." >&2
    exit 1
fi

echo "Verified Developer ID signature metadata enforcement."
