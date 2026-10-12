#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

# shellcheck disable=SC1091
source NativeBuild.env

IFS=, read -r -a SLICES <<<"$GHOSTTY_BUILD_APPLE_SLICES"
SLICE=${2:-macos-arm64}
case ",$GHOSTTY_BUILD_APPLE_SLICES," in
    *,"$SLICE",*) ;;
    *)
        echo "[!] unsupported Apple slice: $SLICE" >&2
        exit 1
        ;;
esac
case "$SLICE" in
    macos-arm64*)
        SLICE_TARGET=$GHOSTTY_BUILD_MACOS_TARGET
        SLICE_I18N=$GHOSTTY_BUILD_MACOS_I18N
        ;;
    ios-arm64*)
        SLICE_TARGET=$GHOSTTY_BUILD_IOS_TARGET
        SLICE_I18N=$GHOSTTY_BUILD_IOS_I18N
        ;;
    *)
        echo "[!] Apple slice has no platform build values: $SLICE" >&2
        exit 1
        ;;
esac
case "$SLICE" in
    *-arm64) ;;
    *-arm64-simulator) SLICE_TARGET+=-simulator ;;
    *)
        echo "[!] unsupported Apple slice variant: $SLICE" >&2
        exit 1
        ;;
esac

GHOSTTY_BUILD_OPTIONS=(
    optimize="$GHOSTTY_BUILD_OPTIMIZE"
    cpu="$GHOSTTY_BUILD_CPU"
    target="$SLICE_TARGET"
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
    i18n="$SLICE_I18N"
)

case "${1:-}" in
    arguments)
        printf '%s\n' "${GHOSTTY_BUILD_OPTIONS[@]/#/-D}"
        ;;
    vector)
        (IFS=,; printf '%s\n' "${GHOSTTY_BUILD_OPTIONS[*]}")
        ;;
    target)
        printf '%s\n' "$SLICE_TARGET"
        ;;
    slices)
        printf '%s\n' "${SLICES[@]}"
        ;;
    *)
        echo "Usage: $0 <arguments|vector|target|slices> [slice]" >&2
        exit 1
        ;;
esac
