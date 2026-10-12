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

signature="$manifest.asc"
trap 'rm -f -- "$signature.part"' EXIT
gpg --batch --no-tty --no-auto-key-retrieve --yes --pinentry-mode error \
    --local-user "$fingerprint!" \
    --output "$signature.part" \
    --armor --detach-sign "$manifest"
mv -f -- "$signature.part" "$signature"
echo "Release manifest signed by $fingerprint"
