#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <macosx|iphoneos|iphonesimulator> <binary> <link-map>" >&2
    exit 64
fi

scripts=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$scripts/apple-platform.sh"
kodosi_apple_platform "$1"
platform=$1
binary=$2
link_map=$3
repository=$(kodosi_repository_root)
ghostty=${KODOSI_GHOSTTY_DIR:-$repository/terminal/ghostty}
if [[ $platform == macosx ]]; then
    metadata="$ghostty/Vendor/libghostty.version"
    symbol_manifest="$ghostty/Vendor/symbols/combined-strong.txt"
    app=$(dirname "$(dirname "$(dirname "$binary")")")
else
    metadata="$ghostty/Vendor/libghostty-$KODOSI_GHOSTTY_SLICE.version"
    symbol_manifest="$ghostty/Vendor/symbols/$KODOSI_GHOSTTY_SLICE-strong.txt"
    app=$(dirname "$binary")
fi
if [[ ${KODOSI_WORKING_TREE_VALIDATION:-0} == 1 ]]; then
    record="combined_archive_sha256 in $metadata"
    expected=$(sed -n 's/^combined_archive_sha256=//p' "$metadata")
else
    key="${KODOSI_GHOSTTY_SLICE//-/_}_archive_sha256"
    record="$key in Ghostty.lock"
    expected=$(sed -n "s/^$key=//p" "$repository/Ghostty.lock")
fi

fail() {
    echo "The $platform app $binary $1" >&2
    exit 1
}

architectures=$(lipo -archs "$binary")
[[ "$architectures" == arm64 ]] || fail "has the architectures $architectures. It must have only arm64."

linked=$(LC_ALL=C awk '
    /^\[[[:space:]]*[0-9]+\][[:space:]]+\/.*libghostty\.a\(/ {
        row = $0
        sub(/^\[[[:space:]]*[0-9]+\][[:space:]]+/, "", row)
        sub(/\(.*/, "", row)
        print row
    }
' "$link_map" | LC_ALL=C sort -u)
count=$(grep -c . <<<"$linked" || true)
if [[ "$count" -ne 1 ]]; then
    printf '%s\n' "$linked" >&2
    fail "links $count Ghostty archives. It must link exactly one."
fi
digest=$(shasum -a 256 "$linked" | awk '{print $1}')
[[ "$digest" == "$expected" ]] || fail "links $linked. Its SHA-256 is $digest, but $record is $expected."

python3 "$scripts/verify-ghostty-link-provenance.py" "$link_map" "$symbol_manifest" "$linked" "$binary"
for symbol in \
    _ghostty_surface_new \
    _ghostty_surface_checkpoint_restore \
    _ghostty_surface_set_display_id \
    _ghostty_terminal_new \
    _ghostty_terminal_vt_write; do
    grep -Fq "$symbol" "$link_map" || fail "does not link the Ghostty symbol $symbol."
done
if nm -u "$binary" | grep -Eq '(^|[[:space:]])_ghostty_'; then
    fail "has Ghostty symbols that the link did not resolve."
fi
if find "$app" -type f \( -name 'libghostty*.dylib' -o -path '*Ghostty*.framework/*' \) -print -quit | grep -q .; then
    fail "is in an app with a dynamic Ghostty image."
fi
later=$(kodosi_later_deployment_targets "$binary")
[[ -z "$later" ]] || fail "needs $platform $(xargs <<<"$later"). The deployment target is $KODOSI_DEPLOYMENT_TARGET."

echo "Verified that the $platform app $binary links one Ghostty image, and that it agrees with $record."
