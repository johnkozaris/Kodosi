#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFIER="$ROOT/scripts/release/verify-publishable.sh"
SCRATCH="$ROOT/build/publishability-verifier-test"
APP="$SCRATCH/KodosiDesktop.app"
INFO="$APP/Contents/Info.plist"
RESOURCES="$APP/Contents/Resources"
MARKER=KodosiNonPublishableBuild

trap 'rm -rf "$SCRATCH"' EXIT
rm -rf "$SCRATCH"
mkdir -p "$RESOURCES"
write_info() {
    /usr/bin/plutil -create xml1 "$INFO"
    /usr/bin/plutil -insert CFBundleIdentifier -string com.kodosi.desktop "$INFO"
    /usr/bin/plutil -insert "$MARKER" -string "$1" "$INFO"
}

write_info NO
"$VERIFIER" "$APP"
write_info working-tree-validation
if "$VERIFIER" "$APP" >"$SCRATCH/plist.log" 2>&1; then
    echo "Publishability verifier accepted a diagnostic Info.plist marker." >&2
    exit 1
fi
grep -F "Refusing non-publishable Kodosi artifact" "$SCRATCH/plist.log" >/dev/null

write_info NO
printf 'working-tree-validation\n' >"$RESOURCES/$MARKER"
if "$VERIFIER" "$APP" >"$SCRATCH/resource.log" 2>&1; then
    echo "Publishability verifier accepted a diagnostic resource marker." >&2
    exit 1
fi
grep -F "resource marker" "$SCRATCH/resource.log" >/dev/null
printf 'Verified non-publishable artifact rejection.\n'
