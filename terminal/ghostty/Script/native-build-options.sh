#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source NativeBuild.env

GHOSTTY_BUILD_OPTIONS=(
    optimize="$GHOSTTY_BUILD_OPTIMIZE"
    cpu="$GHOSTTY_BUILD_CPU"
    target="$GHOSTTY_BUILD_TARGET"
    emit-exe="$GHOSTTY_BUILD_EMIT_EXE"
    emit-macos-app="$GHOSTTY_BUILD_EMIT_MACOS_APP"
    emit-docs="$GHOSTTY_BUILD_EMIT_DOCS"
    emit-themes="$GHOSTTY_BUILD_EMIT_THEMES"
    emit-webdata="$GHOSTTY_BUILD_EMIT_WEBDATA"
    sentry="$GHOSTTY_BUILD_SENTRY"
    emit-lib-vt="$GHOSTTY_BUILD_EMIT_LIB_VT"
    emit-combined-lib="$GHOSTTY_BUILD_EMIT_COMBINED_LIB"
    app-runtime="$GHOSTTY_BUILD_APP_RUNTIME"
    emit-xcframework="$GHOSTTY_BUILD_EMIT_XCFRAMEWORK"
    custom-shaders="$GHOSTTY_BUILD_CUSTOM_SHADERS"
    inspector="$GHOSTTY_BUILD_INSPECTOR"
)

case "${1:-}" in
    arguments)
        printf '%s\n' "${GHOSTTY_BUILD_OPTIONS[@]/#/-D}"
        ;;
    vector)
        (IFS=,; printf '%s\n' "${GHOSTTY_BUILD_OPTIONS[*]}")
        ;;
    *)
        echo "Usage: $0 <arguments|vector>" >&2
        exit 1
        ;;
esac
