#!/usr/bin/env bash

set -euo pipefail

folder=${1:?Usage: make-signing-certificate.sh <new folder>}

umask 077
mkdir "$folder"
cat > "$folder/openssl.cnf" <<'EOF'
[req]
prompt = no
distinguished_name = subject
x509_extensions = extensions

[subject]
CN = Kodosi

[extensions]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature
extendedKeyUsage = codeSigning
EOF
openssl req -x509 -config "$folder/openssl.cnf" -newkey rsa:4096 -sha256 -days 7305 -nodes \
    -keyout "$folder/Kodosi.key" -out "$folder/Kodosi.cer"
openssl pkcs12 -export -name Kodosi -inkey "$folder/Kodosi.key" -in "$folder/Kodosi.cer" \
    -out "$folder/Kodosi.pfx" -passout pass:
rm "$folder/Kodosi.key" "$folder/openssl.cnf"
