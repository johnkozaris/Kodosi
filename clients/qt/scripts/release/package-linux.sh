#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

if [[ $# -ne 2 ]]; then
    echo "Usage: package-linux.sh <build-dir> <output-dir>" >&2
    exit 2
fi
build_dir=$(realpath "$1")
mkdir -p "$2"
output_dir=$(realpath "$2")
version=$(python3 scripts/release/product-version.py)
epoch=${SOURCE_DATE_EPOCH:-$(python3 scripts/release/source-date-epoch.py --source-root .)}
if [[ ! "$epoch" =~ ^[0-9]+$ ]] || (( epoch <= 0 )); then
    echo "SOURCE_DATE_EPOCH must be a positive integer" >&2
    exit 2
fi
export SOURCE_DATE_EPOCH=$epoch

tree="kodosi-${version}-Linux"
archive="$output_dir/kodosi-${version}-linux-x86_64.tar.gz"
package="$output_dir/kodosi-bin-${version}-1-x86_64.pkg.tar.zst"
mkdir -p build
stage=$(mktemp -d "$PWD/build/package-linux.XXXXXX")
trap 'rm -rf "$stage" "$output_dir/_CPack_Packages"' EXIT

rm -f "$output_dir"/kodosi_*.deb \
    "$output_dir"/kodosi-*-linux-x86_64.tar.gz \
    "$output_dir"/kodosi-bin-*.pkg.tar.zst
umask 022
.tools/cmake/bin/cpack --config "$build_dir/CPackConfig.cmake" -B "$output_dir"

normalize_modes() {
    find "$1" -type d -exec chmod 0755 {} +
    while IFS= read -r -d '' path; do
        if [[ -x "$path" ]]; then
            chmod 0755 "$path"
        else
            chmod 0644 "$path"
        fi
    done < <(find "$1" -type f -print0)
}

tar -xzf "$archive" -C "$stage"
test -d "$stage/$tree/usr"
normalize_modes "$stage/$tree"
find "$stage/$tree" -print0 |
    xargs -0 touch -h --date="@$epoch"
tar \
    --sort=name \
    --format=gnu \
    --mtime="@$epoch" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -cf - \
    -C "$stage" "$tree" |
    gzip -n -9 >"$stage/archive.tar.gz"
mv -f "$stage/archive.tar.gz" "$archive"

mkdir "$stage/arch" "$stage/normalized"
tar -xzf "$archive" -C "$stage/normalized"
cp -a "$stage/normalized/$tree/usr" "$stage/arch/"
normalize_modes "$stage/arch"
installed_size=$(du -sb "$stage/arch/usr" | awk '{ print $1 }')
cat >"$stage/arch/.PKGINFO" <<EOF
pkgname = kodosi-bin
pkgbase = kodosi-bin
pkgver = ${version}-1
pkgdesc = Shared terminals and rooms for people and their coding agents
url = https://kodosi.com
builddate = ${epoch}
packager = Kodosi Release <support@kodosi.com>
size = ${installed_size}
arch = x86_64
license = Apache-2.0
depend = alsa-lib
depend = at-spi2-core
depend = brotli
depend = cairo
depend = dbus
depend = desktop-file-utils
depend = expat
depend = libdrm
depend = libglvnd
depend = fontconfig
depend = freetype2
depend = mesa
depend = gdk-pixbuf2
depend = krb5
depend = gtk3
depend = harfbuzz
depend = nspr
depend = nss
depend = pango
depend = systemd-libs
depend = wayland
depend = libx11
depend = libxcb
depend = libxcomposite
depend = libxdamage
depend = libxext
depend = libxfixes
depend = libxkbfile
depend = libxrandr
depend = libxtst
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
chmod 0644 "$stage/arch/.PKGINFO"
find "$stage/arch" -print0 |
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
    -C "$stage/arch" .PKGINFO usr |
    zstd --quiet --threads=1 -19 -o "$stage/package.pkg.tar.zst"
mv -f "$stage/package.pkg.tar.zst" "$package"
echo "Linux packages written to $output_dir"
