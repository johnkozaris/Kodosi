#!/usr/bin/env bash

set -euo pipefail

usage="Usage: bundle-msix.sh <bundle.msixbundle> <package.msix>..."
bundle=${1:?$usage}
shift
if (($# == 0)); then
    echo "$usage" >&2
    exit 2
fi
version=$(python3 "$(dirname "$0")/product-version.py")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export MSYS_NO_PATHCONV=1

cp "$@" "$work/"
rm -f "$bundle"
makeappx.exe bundle /d "$(cygpath -w "$work")" /p "$(cygpath -w "$bundle")" /bv "$version.0" /o
