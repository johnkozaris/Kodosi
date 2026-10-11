#!/usr/bin/env bash

set -euo pipefail

root=${1:?Usage: verify-installed-package-content.sh <root>}
cd "$(dirname "$0")/../.."
root=$(realpath "$root")

test -x "$root/usr/bin/kodosi"
test -x "$root/usr/bin/kodosi-qt"
test -x "$root/usr/lib/kodosi/bin/kodosi"
test -f "$root/usr/share/applications/com.kodosi.Kodosi.desktop"
grep -Fxq 'Exec=kodosi-qt %u' \
    "$root/usr/share/applications/com.kodosi.Kodosi.desktop"
grep -Fxq 'MimeType=x-scheme-handler/kodosi;' \
    "$root/usr/share/applications/com.kodosi.Kodosi.desktop"
readelf -h "$root/usr/lib/kodosi/bin/kodosi" | grep -F 'ELF64' >/dev/null
if ldd "$root/usr/lib/kodosi/bin/kodosi" | grep -Fq 'not found'; then
    echo "Bundled Kodosi CLI has unresolved ELF dependencies" >&2
    exit 1
fi

cmp dependencies.lock.json \
    "$root/usr/share/doc/kodosi/provenance/dependencies.lock.json"
python3 scripts/release/verify-source-identity.py \
    --identity "$root/usr/share/doc/kodosi/provenance/source-identity.json" \
    --expected build/release/generated/source-identity.json
cmp ../../terminal/ghostty/LICENSE \
    "$root/usr/share/doc/kodosi/ghostty/LICENSE"
cmp ../../terminal/ghostty/LICENSE-GHOSTTY \
    "$root/usr/share/doc/kodosi/ghostty/LICENSE-GHOSTTY"
cmp ../../terminal/ghostty/THIRD_PARTY_NOTICES.md \
    "$root/usr/share/doc/kodosi/ghostty/THIRD_PARTY_NOTICES.md"
cmp ../../terminal/ghostty/ThirdPartyNotices/linux-vt-inventory.json \
    "$root/usr/share/doc/kodosi/ghostty/linux-vt-inventory.json"
cmp ../../terminal/ghostty/LinuxGhostty.ref \
    "$root/usr/share/doc/kodosi/ghostty/LinuxGhostty.ref"
python3 scripts/release/verify-rust-license-inventory.py \
    --tree "$root/usr/share/doc/kodosi/rust" \
    --expected-tree build/release/generated/rust-dependency-licenses
python3 scripts/release/verify-native-license-evidence.py \
    --installed-root "$root"

notice_names=(
    highway-Apache-2.0.txt
    highway-BSD-3-Clause.txt
    simdutf-MIT.txt
    simdutf-PyTorch-BSD-3-Clause.txt
    simdutf-Fuchsia-BSD-3-Clause.txt
    uucode.txt
    uucode-Bjoern-Hoehrmann.txt
    unicode-data.txt
    wuffs.txt
    zig-compiler-runtime.txt
    compiler-rt-LLVM-derived.txt
    zig-runtime-Apache-2.0-LLVM-exception.txt
    compiler-rt-musl-derived.txt
    zig-runtime-MIT-derived.txt
)
for notice in "${notice_names[@]}"; do
    cmp "../../terminal/ghostty/ThirdPartyNotices/licenses/$notice" \
        "$root/usr/share/doc/kodosi/ghostty/ThirdPartyNotices/licenses/$notice"
done

cmp packaging/linux/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt \
    "$root/usr/share/doc/kodosi/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt"
