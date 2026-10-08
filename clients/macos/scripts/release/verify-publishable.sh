#!/usr/bin/env bash
set -euo pipefail

artifact=${1:?usage: verify-publishable.sh <app-or-archive>}
marker_key=KodosiNonPublishableBuild

if [[ -d "$artifact/Products/Applications/KodosiDesktop.app" ]]; then
    app="$artifact/Products/Applications/KodosiDesktop.app"
elif [[ -d "$artifact/Contents" ]]; then
    app="$artifact"
else
    echo "Kodosi app artifact not found: $artifact" >&2
    exit 1
fi

info_plist="$app/Contents/Info.plist"
resources="$app/Contents/Resources"
if [[ ! -f "$info_plist" ]]; then
    echo "Kodosi app Info.plist not found: $info_plist" >&2
    exit 1
fi

marker=$(/usr/libexec/PlistBuddy -c "Print :$marker_key" "$info_plist" 2>/dev/null || true)
if [[ -n "$marker" && "$marker" != NO && "$marker" != 0 ]]; then
    echo "Refusing non-publishable Kodosi artifact: Info.plist $marker_key=$marker" >&2
    exit 1
fi
if [[ -e "$resources/$marker_key" ]]; then
    echo "Refusing non-publishable Kodosi artifact: resource marker $resources/$marker_key" >&2
    exit 1
fi
