#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source LinuxVtBuild.env

OPTIONS=(
    "emit-lib-vt=true"
    "target=$GHOSTTY_LINUX_VT_TARGET"
    "cpu=$GHOSTTY_LINUX_VT_CPU"
    "optimize=$GHOSTTY_LINUX_VT_OPTIMIZE"
    "strip=true"
)

case "${1:-}" in
    arguments)
        printf '%s\n' "${OPTIONS[@]/#/-D}"
        ;;
    vector)
        (IFS=,; printf '%s\n' "${OPTIONS[*]}")
        ;;
    *)
        echo "Usage: $0 <arguments|vector>" >&2
        exit 1
        ;;
esac
