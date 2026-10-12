#!/usr/bin/env bash

set -euo pipefail

manifest=${1:?Usage: sign-release-manifest.sh <release-manifest.json> [fingerprint]}
fingerprint=${2:-${KODOSI_GPG_FINGERPRINT:-}}
if [[ -z "$fingerprint" ]]; then
    echo "A trusted signing fingerprint is required." >&2
    exit 2
fi
if ! command -v gpg >/dev/null 2>&1; then
    echo "gpg is required for optional release signing." >&2
    exit 2
fi
manifest=$(realpath "$manifest")
if [[ $(basename "$manifest") != release-manifest.json || -L "$manifest" || ! -f "$manifest" ]]; then
    echo "Only an exact regular release-manifest.json can be signed." >&2
    exit 2
fi
if [[ ! "$fingerprint" =~ ^[0-9A-Fa-f]{40}$ ]]; then
    echo "The trusted fingerprint must contain exactly 40 hexadecimal characters." >&2
    exit 2
fi
fingerprint=${fingerprint^^}
script_dir=$(cd "$(dirname "$0")" && pwd)
python3 "$script_dir/verify-release-manifest.py" \
    --source-root "${KODOSI_SOURCE_ROOT:-$script_dir/../..}" \
    --manifest "$manifest" \
    --output-dir "$(dirname "$manifest")" \
    --dependencies \
        "${KODOSI_DEPENDENCIES:-$script_dir/../../dependencies.lock.json}" \
    >/dev/null

secret_fingerprints=$(
    gpg --batch --no-tty --no-auto-key-retrieve \
        --with-colons --list-secret-keys "$fingerprint" 2>/dev/null |
        awk -F: '$1 == "fpr" { print toupper($10) }'
)
if ! grep -Fxq "$fingerprint" <<<"$secret_fingerprints"; then
    echo "The exact trusted secret-key fingerprint is unavailable." >&2
    exit 1
fi

signature="$manifest.asc"
rm -f -- "$signature"
gpg --batch --no-tty --no-auto-key-retrieve --yes --pinentry-mode error \
    --local-user "$fingerprint!" \
    --output "$signature" \
    --armor --detach-sign "$manifest"
test -s "$signature"
grep -Fxq -- '-----BEGIN PGP SIGNATURE-----' <(head -n 1 "$signature")
"$script_dir/verify-release-signature.sh" \
    "$manifest" "$signature" "$fingerprint"
echo "Release manifest signed by $fingerprint"
