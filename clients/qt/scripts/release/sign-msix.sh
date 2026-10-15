#!/usr/bin/env bash

set -euo pipefail

package=${1:?Usage: sign-msix.sh <package.msix>}
dlib=${KODOSI_SIGNING_DLIB:?Set KODOSI_SIGNING_DLIB to Azure.CodeSigning.Dlib.dll}
metadata=${KODOSI_SIGNING_METADATA:?Set KODOSI_SIGNING_METADATA to the signing metadata.json}
export MSYS_NO_PATHCONV=1

block_map_digest() {
    python3 -c 'import hashlib, sys, zipfile; print(hashlib.sha256(zipfile.ZipFile(sys.argv[1]).read("AppxBlockMap.xml")).hexdigest())' "$1"
}

before=$(block_map_digest "$package")
signtool.exe sign /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 \
    /dlib "$(cygpath -w "$dlib")" /dmdf "$(cygpath -w "$metadata")" "$(cygpath -w "$package")"
if [ "$(block_map_digest "$package")" != "$before" ]; then
    echo "Signing changed the package payload" >&2
    exit 1
fi
signtool.exe verify /pa "$(cygpath -w "$package")"
