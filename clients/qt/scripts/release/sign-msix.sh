#!/usr/bin/env bash

set -euo pipefail

package=${1:?Usage: sign-msix.sh <package.msix or bundle.msixbundle>}
certificate=${KODOSI_SIGNING_CERTIFICATE:-$(dirname "$0")/../../packaging/windows/Kodosi.cer}
export MSYS_NO_PATHCONV=1

payload_digest() {
    python3 -c '
import hashlib, io, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as outer:
    packages = sorted(name for name in outer.namelist() if name.endswith(".msix"))
    maps = [zipfile.ZipFile(io.BytesIO(outer.read(name))).read("AppxBlockMap.xml") for name in packages]
    print(hashlib.sha256(b"".join(maps or [outer.read("AppxBlockMap.xml")])).hexdigest())
' "$(cygpath -w "$package")"
}

thumbprint=$(python3 -c '
import hashlib, ssl, sys
print(hashlib.sha1(ssl.PEM_cert_to_DER_cert(open(sys.argv[1], encoding="ascii").read())).hexdigest())
' "$(cygpath -w "$certificate")")
before=$(payload_digest)
signtool.exe sign /fd SHA256 /sha1 "$thumbprint" /tr http://timestamp.acs.microsoft.com /td SHA256 \
    "$(cygpath -w "$package")"
if [ "$(payload_digest)" != "$before" ]; then
    echo "Signing changed the package payload" >&2
    exit 1
fi
