#!/usr/bin/env bash
# Puts the x64 and ARM64 packages of one version in one MSIX bundle. Sign the
# bundle, not the packages. Run after kodosi_windows_env, which puts the Windows
# SDK tools on PATH.

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
# The SDK tools take /switches, which Git Bash would otherwise rewrite as paths.
export MSYS_NO_PATHCONV=1

cp "$@" "$work/"
rm -f "$bundle"
makeappx.exe bundle /d "$(cygpath -w "$work")" /p "$(cygpath -w "$bundle")" /bv "$version.0" /o
