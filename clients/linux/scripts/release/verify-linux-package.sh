#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

package=${1:?Usage: verify-linux-package.sh <package.deb>}
package=$(realpath "$package")
stage="$PWD/build/package-verify"
rm -rf "$stage"
mkdir -p "$stage"
trap 'rm -rf "$stage"' EXIT

dpkg-deb -x "$package" "$stage"
dependencies=$(dpkg-deb -f "$package" Depends)
if ! grep -Eq '(^|, )libopengl0(,|$)' <<<"$dependencies"; then
    echo "DEB does not declare the host OpenGL loader dependency" >&2
    exit 1
fi
if ! grep -Eq '(^|, )desktop-file-utils(,|$)' <<<"$dependencies" ||
    ! grep -Eq '(^|, )xdg-desktop-portal(,|$)' <<<"$dependencies" ||
    ! grep -Eq '(^|, )xdg-utils(,|$)' <<<"$dependencies"; then
    echo "DEB does not declare portal and desktop-handler dependencies" >&2
    exit 1
fi
if grep -Fq 'libxcb-icccm4 (' <<<"$dependencies" ||
    grep -Fq 'libxcb-keysyms1 (' <<<"$dependencies"; then
    echo "DEB contains host-derived XCB version floors" >&2
    exit 1
fi

test -x "$stage/usr/lib/kodosi/bin/kodosi-qt"
test -f "$stage/usr/lib/kodosi/bin/qt.conf"
test -f "$stage/usr/share/metainfo/com.kodosi.Kodosi.metainfo.xml"
test -f "$stage/usr/share/icons/hicolor/scalable/apps/com.kodosi.Kodosi.svg"
test -f "$stage/usr/lib/kodosi/lib/libQt6Core.so.6.11.2"
test -f "$stage/usr/lib/kodosi/lib/libQt6DBus.so.6.11.2"
test -f "$stage/usr/lib/kodosi/lib/libQt6Widgets.so.6.11.2"
test -f "$stage/usr/lib/kodosi/plugins/platforms/libqxcb.so"
test -f "$stage/usr/lib/kodosi/plugins/platforms/libqwayland.so"
test -f "$stage/usr/lib/kodosi/plugins/platforms/libqoffscreen.so"
test -f "$stage/usr/lib/kodosi/plugins/platformthemes/libqgtk3.so"
test -f "$stage/usr/lib/kodosi/plugins/platformthemes/libqxdgdesktopportal.so"
test -f "$stage/usr/lib/kodosi/qml/QtQuick/Controls/libqtquickcontrols2plugin.so"
if find "$stage/usr/lib" -maxdepth 1 \
    \( -name 'libQt6*' -o -name 'libicu*' \) -print -quit | grep -q .; then
    echo "Bundled runtime escaped /usr/lib/kodosi" >&2
    exit 1
fi

verify_private_linkage() {
    local binary=$1
    local output
    output=$(env -u LD_LIBRARY_PATH ldd "$binary")
    if grep -Fq 'not found' <<<"$output"; then
        echo "Unresolved packaged dependency in $binary" >&2
        printf '%s\n' "$output" >&2
        exit 1
    fi
    while IFS= read -r line; do
        case "$line" in
            *libQt6* | *libicu*)
                path=$(awk '{ print $3 }' <<<"$line")
                if [[ "$path" != "$stage/usr/lib/kodosi/"* ]]; then
                    echo "Packaged dependency escaped private runtime: $line" >&2
                    exit 1
                fi
                ;;
        esac
    done <<<"$output"
}

verify_private_linkage "$stage/usr/lib/kodosi/bin/kodosi-qt"
verify_private_linkage "$stage/usr/lib/kodosi/plugins/platforms/libqxcb.so"
verify_private_linkage "$stage/usr/lib/kodosi/plugins/platforms/libqwayland.so"
verify_private_linkage "$stage/usr/lib/kodosi/plugins/platforms/libqoffscreen.so"
verify_private_linkage "$stage/usr/lib/kodosi/plugins/platformthemes/libqgtk3.so"
verify_private_linkage \
    "$stage/usr/lib/kodosi/plugins/platformthemes/libqxdgdesktopportal.so"
verify_private_linkage \
    "$stage/usr/lib/kodosi/qml/QtQuick/Controls/libqtquickcontrols2plugin.so"

desktop-file-validate \
    "$stage/usr/share/applications/com.kodosi.Kodosi.desktop"
update-desktop-database "$stage/usr/share/applications"
grep -Eq '^x-scheme-handler/kodosi=.*com\.kodosi\.Kodosi\.desktop' \
    "$stage/usr/share/applications/mimeinfo.cache"
appstreamcli validate --no-net \
    "$stage/usr/share/metainfo/com.kodosi.Kodosi.metainfo.xml"

./scripts/release/verify-installed-package-content.sh "$stage"

echo "Linux package verified: $(basename "$package")"
