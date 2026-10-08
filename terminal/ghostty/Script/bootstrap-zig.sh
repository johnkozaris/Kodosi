#!/bin/zsh

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source Toolchain.env

TOOLCHAIN_ROOT="$PWD/.tools/zig-$ZIG_VERSION-$ZIG_SHA256"
DOWNLOAD_ROOT="$PWD/.tools/downloads"
ARCHIVE="$DOWNLOAD_ROOT/zig-$ZIG_TARGET-$ZIG_VERSION.tar.xz"
ZIG_BIN="$TOOLCHAIN_ROOT/zig"
PROVENANCE_FILE="$TOOLCHAIN_ROOT/.provenance-key"
PROVENANCE_KEY="$ZIG_VERSION:$ZIG_TARGET:$ZIG_SHA256"

if [ -x "$ZIG_BIN" ] &&
    [ -f "$PROVENANCE_FILE" ] &&
    [ "$(<"$PROVENANCE_FILE")" = "$PROVENANCE_KEY" ] &&
    [ "$("$ZIG_BIN" version)" = "$ZIG_VERSION" ]; then
    printf '%s\n' "$ZIG_BIN"
    exit 0
fi

mkdir -p "$DOWNLOAD_ROOT"
if [ ! -f "$ARCHIVE" ] ||
    ! printf '%s  %s\n' "$ZIG_SHA256" "$ARCHIVE" | shasum -a 256 -c - >/dev/null 2>&1; then
    echo "[*] downloading isolated Zig $ZIG_VERSION" >&2
    temporary="$ARCHIVE.tmp.$$"
    trap 'rm -f "$temporary"' EXIT
    curl -fL --retry 3 -o "$temporary" "$ZIG_URL"
    printf '%s  %s\n' "$ZIG_SHA256" "$temporary" | shasum -a 256 -c - >&2
    mv "$temporary" "$ARCHIVE"
    trap - EXIT
fi

STAGE="$TOOLCHAIN_ROOT.stage.$$"
rm -rf "$STAGE"
mkdir -p "$STAGE"
tar -xJf "$ARCHIVE" -C "$STAGE" --strip-components=1

if [ "$("$STAGE/zig" version)" != "$ZIG_VERSION" ]; then
    echo "[!] isolated Zig toolchain failed validation" >&2
    exit 1
fi

rm -rf "$TOOLCHAIN_ROOT"
mv "$STAGE" "$TOOLCHAIN_ROOT"
printf '%s\n' "$PROVENANCE_KEY" > "$PROVENANCE_FILE"
printf '%s\n' "$ZIG_BIN"
