#!/usr/bin/env bash

set -euo pipefail

layout=${1:?Usage: package-msix.sh <layout> <package.msix>}
package=${2:?Usage: package-msix.sh <layout> <package.msix>}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# The SDK tools take /switches, which Git Bash would otherwise rewrite as paths.
export MSYS_NO_PATHCONV=1

mkdir -p "$work/resources"
cp -R "$layout/Assets" "$work/resources/Assets"
makepri.exe createconfig /cf "$(cygpath -w "$work/priconfig.xml")" /dq en-US /pv 10.0.0 /o
makepri.exe new \
    /pr "$(cygpath -w "$work/resources")" \
    /cf "$(cygpath -w "$work/priconfig.xml")" \
    /mn "$(cygpath -w "$layout/AppxManifest.xml")" \
    /of "$(cygpath -w "$layout/resources.pri")" \
    /o
rm -f "$package"
makeappx.exe pack /d "$(cygpath -w "$layout")" /p "$(cygpath -w "$package")" /h SHA256 /o
