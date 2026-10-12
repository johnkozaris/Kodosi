#!/bin/bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCHIVE="$ROOT/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
INVENTORY="$ROOT/ThirdPartyNotices/inventory.json"

usage() {
    echo "usage: $0 <replacement-libintl.a> <output-libghostty.a> [non-libintl.a]" >&2
    exit 64
}

[ "$#" -ge 2 ] && [ "$#" -le 3 ] || usage
REPLACEMENT="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
OUTPUT="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
NONLIBINTL="${3:-}"

[ -f "$REPLACEMENT" ] || { echo "replacement archive is missing: $REPLACEMENT" >&2; exit 66; }
[ -f "$ARCHIVE" ] || { echo "combined archive is missing: $ARCHIVE" >&2; exit 66; }
[ "$REPLACEMENT" != "$OUTPUT" ] || { echo "replacement and output paths must differ" >&2; exit 64; }

TMP="$(mktemp -d "${TMPDIR:-/tmp}/kodosi-libintl-relink.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
cp "$ARCHIVE" "$TMP/non-libintl.a"

python3 - "$INVENTORY" > "$TMP/libintl-members.txt" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1]))
members = [
    record["name"]
    for record in manifest["archiveMembers"]
    if "gettext-libintl" in record.get("owners", [])
]
if len(members) != 30 or len(members) != len(set(members)):
    raise SystemExit(f"expected 30 unique libintl members, found {len(members)}")
print("\n".join(members))
PY

while IFS= read -r member; do
    ar -d "$TMP/non-libintl.a" "$member"
done < "$TMP/libintl-members.txt"
ranlib "$TMP/non-libintl.a"

if [ -n "$NONLIBINTL" ]; then
    mkdir -p "$(dirname "$NONLIBINTL")"
    cp "$TMP/non-libintl.a" "$NONLIBINTL"
fi

mkdir "$TMP/replacement"
(
    cd "$TMP/replacement"
    ar -x "$REPLACEMENT"
)
replacement_count="$(ar -t "$REPLACEMENT" | grep -vc '^__.SYMDEF' || true)"
[ "$replacement_count" -gt 0 ] || {
    echo "replacement archive has no object members" >&2
    exit 1
}
extracted_count="$(find "$TMP/replacement" -maxdepth 1 -type f -name '*.o' | wc -l | tr -d ' ')"
[ "$extracted_count" -eq "$replacement_count" ] || {
    echo "replacement archive member extraction was incomplete ($extracted_count/$replacement_count)" >&2
    exit 1
}
find "$TMP/replacement" -maxdepth 1 -type f -name '*.o' -exec chmod 0644 {} +

mkdir -p "$(dirname "$OUTPUT")"
/usr/bin/libtool -static -o "$OUTPUT" "$TMP/non-libintl.a" "$TMP/replacement"/*.o

for member in $(cat "$TMP/libintl-members.txt"); do
    ar -t "$TMP/non-libintl.a" | grep -Fx "$member" >/dev/null && {
        echo "libintl member remained in non-libintl archive: $member" >&2
        exit 1
    }
done

lipo -archs "$OUTPUT" | grep -Eq '(^| )arm64($| )' || {
    echo "relinked archive is not arm64" >&2
    exit 1
}
nm -gU "$OUTPUT" | grep -E '[[:space:]]_libintl_version$' >/dev/null || {
    echo "relinked archive does not provide libintl" >&2
    exit 1
}
nm -gU "$OUTPUT" | grep -E '[[:space:]]_ghostty_init$' >/dev/null || {
    echo "relinked archive does not provide Ghostty" >&2
    exit 1
}

echo "created relinked archive: $OUTPUT"
[ -z "$NONLIBINTL" ] || echo "created non-libintl object archive: $NONLIBINTL"
