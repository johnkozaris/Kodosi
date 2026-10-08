#!/usr/bin/env bash

set -euo pipefail

manifest=${1:?Usage: verify-release-signature.sh <release-manifest.json> <signature.asc> <fingerprint>}
signature=${2:?Usage: verify-release-signature.sh <release-manifest.json> <signature.asc> <fingerprint>}
fingerprint=${3:?Usage: verify-release-signature.sh <release-manifest.json> <signature.asc> <fingerprint>}
if ! command -v gpg >/dev/null 2>&1; then
    echo "gpg is required for release signature verification." >&2
    exit 2
fi
manifest=$(realpath "$manifest")
signature=$(realpath "$signature")
if [[ $(basename "$manifest") != release-manifest.json || -L "$manifest" || ! -f "$manifest" ]]; then
    echo "The manifest path is invalid." >&2
    exit 2
fi
if [[ "$signature" != "$manifest.asc" || -L "$signature" || ! -f "$signature" ]]; then
    echo "The detached signature path is invalid." >&2
    exit 2
fi
if [[ ! "$fingerprint" =~ ^[0-9A-Fa-f]{40}$ ]]; then
    echo "The trusted fingerprint must contain exactly 40 hexadecimal characters." >&2
    exit 2
fi
fingerprint=${fingerprint^^}
script_dir=$(cd "$(dirname "$0")" && pwd)
python3 "$script_dir/verify-release-manifest.py" \
    --manifest "$manifest" \
    --structure-only >/dev/null

if ! status=$(
    gpg --batch --no-tty --no-auto-key-retrieve --status-fd=1 \
        --verify "$signature" "$manifest" 2>/dev/null
); then
    echo "The detached signature is invalid." >&2
    exit 1
fi
if grep -Eq \
    '^\[GNUPG:\] (BADSIG|ERRSIG|NO_PUBKEY|NODATA|EXPKEYSIG|EXPSIG|REVKEYSIG|KEYEXPIRED|KEYREVOKED|SIGEXPIRED)( |$)' \
    <<<"$status"; then
    echo "The detached signature status is invalid, expired, or revoked." >&2
    exit 1
fi
valid_signers=$(
    awk '$1 == "[GNUPG:]" && $2 == "VALIDSIG" {
        print toupper($3) ":" toupper($12)
    }' \
        <<<"$status"
)
if [[ $(grep -c . <<<"$valid_signers") -ne 1 ]]; then
    echo "The detached signature did not produce exactly one valid signature." >&2
    exit 1
fi
signing_fingerprint=${valid_signers%%:*}
primary_fingerprint=${valid_signers#*:}
if [[ ! "$signing_fingerprint" =~ ^[0-9A-F]{40}$ ]] ||
    { [[ -n "$primary_fingerprint" ]] &&
        [[ ! "$primary_fingerprint" =~ ^[0-9A-F]{40}$ ]]; }; then
    echo "The detached signature returned malformed fingerprint status." >&2
    exit 1
fi
if [[ "$signing_fingerprint" != "$fingerprint" &&
      "$primary_fingerprint" != "$fingerprint" ]]; then
    echo "The detached signature does not match the trusted fingerprint." >&2
    exit 1
fi
echo "Release signature verified: $fingerprint"
