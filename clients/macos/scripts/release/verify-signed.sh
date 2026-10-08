#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
cd "$ROOT"

EXPECTED_TEAM_ID=C5886CWK32
: "${KODOSI_TEAM_ID:?Set KODOSI_TEAM_ID to the Apple Developer team identifier}"
if [[ "$KODOSI_TEAM_ID" != "$EXPECTED_TEAM_ID" ]]; then
    echo "KODOSI_TEAM_ID must match the Kodosi product team $EXPECTED_TEAM_ID." >&2
    exit 1
fi

app=${1:-build/archive/KodosiDesktop.xcarchive/Products/Applications/KodosiDesktop.app}
"$ROOT/scripts/release/verify-publishable.sh" "$app"

verify_developer_id_signature() {
    "$ROOT/scripts/release/verify-developer-id-signature.sh" "$1" "$EXPECTED_TEAM_ID"
}

for executable in KodosiDesktop; do
    path="$app/Contents/MacOS/$executable"
    test -x "$path"
    test "$(lipo -archs "$path")" = "arm64"
    deployment_versions=$(otool -l "$path" | awk '
        $1 == "cmd" { build = ($2 == "LC_BUILD_VERSION"); legacy = ($2 == "LC_VERSION_MIN_MACOSX"); next }
        build && $1 == "minos" { print $2; build = 0; next }
        legacy && $1 == "version" { print $2; legacy = 0 }
    ')
    test -n "$deployment_versions"
    if awk -v maximum="$KODOSI_MACOS_DEPLOYMENT_TARGET" '
        function code(v, p) { split(v, p, "."); return (p[1]+0)*1000000+(p[2]+0)*1000+(p[3]+0) }
        code($1) > code(maximum) { exit 1 }
    ' <<<"$deployment_versions"; then :; else
        echo "$path exceeds macOS $KODOSI_MACOS_DEPLOYMENT_TARGET" >&2
        exit 1
    fi
    verify_developer_id_signature "$path"
done
/usr/bin/codesign --verify --deep --strict --verbose=4 "$app"
verify_developer_id_signature "$app"
/usr/bin/codesign --display --verbose=4 "$app" 2>&1
