#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GHOSTTY_DIR="${KODOSI_GHOSTTY_DIR:-"$ROOT/../../terminal/ghostty"}"
BUNDLE="${1:?Pass the app bundle resources directory}"

notices="$BUNDLE/Notices"
source_dir="$BUNDLE/GhosttySource"
staging=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-notices.XXXXXX")
trap 'rm -rf "$staging"' EXIT

mkdir -p "$staging/Notices/Ghostty/Licenses" "$staging/GhosttySource"
cp "$ROOT/../apple/Notices/"*.txt "$staging/Notices/"
cp "$GHOSTTY_DIR/LICENSE" "$staging/Notices/Ghostty/libghostty-spm.txt"
cp "$GHOSTTY_DIR/LICENSE-GHOSTTY" "$staging/Notices/Ghostty/Ghostty.txt"
cp "$GHOSTTY_DIR/THIRD_PARTY_NOTICES.md" "$staging/Notices/Ghostty/Third-party notices.md"
cp "$GHOSTTY_DIR/ThirdPartyNotices/licenses/"*.txt "$staging/Notices/Ghostty/Licenses/"
cp "$GHOSTTY_DIR/ThirdPartyNotices/inventory.json" "$staging/GhosttySource/"
cp "$GHOSTTY_DIR/ThirdPartyNotices/corresponding-source/"*.tar* "$staging/GhosttySource/"

# Leave out the notices and sources of components that the iOS archives do not contain.
while IFS= read -r name; do
    [ -n "$name" ] || continue
    rm -f "$staging/Notices/Ghostty/Licenses/$name" "$staging/GhosttySource/$name"
done < <(python3 - "$GHOSTTY_DIR/ThirdPartyNotices/inventory.json" <<'PY'
import json
import sys
from pathlib import PurePosixPath

manifest = json.load(open(sys.argv[1]))
absent = set.intersection(*(set(s["absentComponents"]) for s in manifest["appleSlices"].values()))
source_root = PurePosixPath("ThirdPartyNotices/corresponding-source")


def files(components):
    names = set()
    for component in components:
        names.update(component.get("notices", []))
        names.update(
            PurePosixPath(path).name
            for path in component.get("sourcePaths", [])
            if PurePosixPath(path).parent == source_root
        )
    return names


components = manifest["components"]
shipped = files(c for c in components if c["id"] not in absent)
print("\n".join(sorted(files(c for c in components if c["id"] in absent) - shipped)))
PY
)

rm -rf "$notices" "$source_dir"
mv "$staging/Notices" "$notices"
mv "$staging/GhosttySource" "$source_dir"
