#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# shellcheck disable=SC1091
source Tools.env

TOOLS="$PWD/.tools"
DOWNLOADS="$TOOLS/downloads"

download() {
    local url=$1 sha256=$2 archive temporary
    archive="$DOWNLOADS/${url##*/}"
    if [[ ! -f $archive ]] || ! printf '%s  %s\n' "$sha256" "$archive" | sha256sum --strict -c - >/dev/null 2>&1; then
        mkdir -p "$DOWNLOADS"
        temporary="$archive.tmp.$$"
        curl -fsSL --retry 3 -o "$temporary" "$url"
        if ! printf '%s  %s\n' "$sha256" "$temporary" | sha256sum --strict -c - >/dev/null 2>&1; then
            rm -f "$temporary"
            echo "[!] $url does not have the pinned SHA-256." >&2
            exit 1
        fi
        mv "$temporary" "$archive"
    fi
    printf '%s\n' "$archive"
}

install_tool() {
    local name=$1 version=$2 url=$3 sha256=$4 program=$5 asks=$6 root archive stage content
    root="$TOOLS/$name"
    if [[ ! -x $root/$program ]] || [[ $("$root/$program" "$asks") != *"$version" ]]; then
        echo "[*] installing $name $version in $TOOLS" >&2
        archive=$(download "$url" "$sha256")
        stage="$root.stage.$$"
        rm -rf "$stage"
        unzip -q "$archive" -d "$stage"
        content=$stage
        if [[ -d $stage/$name ]]; then
            content=$stage/$name
        fi
        if [[ $("$content/$program" "$asks") != *"$version" ]]; then
            rm -rf "$stage"
            echo "[!] $name from $url is not version $version." >&2
            exit 1
        fi
        rm -rf "$root"
        mv "$content" "$root"
        rm -rf "$stage"
    fi
}

install_tool swiftlint "$SWIFTLINT_VERSION" "$SWIFTLINT_URL" "$SWIFTLINT_SHA256" swiftlint version
install_tool swiftformat "$SWIFTFORMAT_VERSION" "$SWIFTFORMAT_URL" "$SWIFTFORMAT_SHA256" swiftformat --version
# Keep bin/xcodegen under its extracted root: XcodeGen loads setting presets from ../share and
# silently omits base build settings if they are missing.
install_tool xcodegen "$XCODEGEN_VERSION" "$XCODEGEN_URL" "$XCODEGEN_SHA256" bin/xcodegen --version
