#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

version=0.1.0
archive=${1:?Usage: write-arch-package.sh <release.tar.gz> [output-dir]}
output_dir=${2:-build/release}
archive=$(realpath "$archive")
mkdir -p "$output_dir"
output_dir=$(realpath "$output_dir")
epoch=${SOURCE_DATE_EPOCH:-$(python3 scripts/release/source-date-epoch.py --source-root .)}
if [[ ! "$epoch" =~ ^[0-9]+$ ]] || (( epoch <= 0 )); then
    echo "SOURCE_DATE_EPOCH must be a positive integer" >&2
    exit 2
fi

stage="$PWD/build/arch-package-writer.$$"
package="$output_dir/kodosi-bin-${version}-1-x86_64.pkg.tar.zst"
temporary="$output_dir/.kodosi-bin-${version}-1-x86_64.pkg.tar.zst.part.$$"
rm -rf "$stage"
rm -f "$temporary"
mkdir -p "$stage/extracted" "$stage/root"
trap 'rm -rf "$stage"; rm -f "$temporary"' EXIT

tar -xzf "$archive" -C "$stage/extracted"
source_root="$stage/extracted/kodosi-${version}-Linux"
test -d "$source_root/usr"
cp -a "$source_root/usr" "$stage/root/"

chmod 0755 "$stage/root"
find "$stage/root" -type d -exec chmod 0755 {} +
while IFS= read -r -d '' path; do
    if [[ -x "$path" ]]; then
        chmod 0755 "$path"
    else
        chmod 0644 "$path"
    fi
done < <(find "$stage/root" -type f -print0)

installed_size=$(du -sb "$stage/root/usr" | awk '{ print $1 }')
cat >"$stage/root/.PKGINFO" <<EOF
pkgname = kodosi-bin
pkgbase = kodosi-bin
pkgver = ${version}-1
pkgdesc = Local terminals with trusted device and friend access
url = https://kodosi.com
builddate = ${epoch}
packager = Kodosi Release <support@kodosi.com>
size = ${installed_size}
arch = x86_64
license = MIT
depend = at-spi2-core
depend = brotli
depend = cairo
depend = dbus
depend = desktop-file-utils
depend = libdrm
depend = libglvnd
depend = fontconfig
depend = freetype2
depend = mesa
depend = gdk-pixbuf2
depend = krb5
depend = gtk3
depend = harfbuzz
depend = pango
depend = wayland
depend = libx11
depend = libxcb
depend = xcb-util-cursor
depend = xcb-util-image
depend = xcb-util-keysyms
depend = xcb-util-renderutil
depend = xcb-util-wm
depend = libxkbcommon-x11
depend = xdg-desktop-portal
depend = xdg-utils
depend = zstd
depend = zlib
provides = kodosi
conflict = kodosi
EOF
chmod 0644 "$stage/root/.PKGINFO"
find "$stage/root" -print0 |
    xargs -0 touch -h --date="@$epoch"

tar \
    --sort=name \
    --format=posix \
    --pax-option=delete=atime,delete=ctime \
    --mtime="@$epoch" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -cf - \
    -C "$stage/root" .PKGINFO usr |
    zstd --quiet --threads=1 -19 -o "$temporary"
mv -f "$temporary" "$package"
echo "$package"
