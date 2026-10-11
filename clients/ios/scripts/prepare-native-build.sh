#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPOSITORY="$(cd "$ROOT/../.." && pwd)"

case "${PLATFORM_NAME:-}" in
    iphoneos|iphonesimulator) platform=$PLATFORM_NAME ;;
    *)
        echo "Unsupported platform for the iOS runtime: ${PLATFORM_NAME:-<unset>}" >&2
        exit 1
        ;;
esac

case "${CONFIGURATION:-}" in
    Debug) profile=dev ;;
    Release) profile=release ;;
    *)
        echo "Unsupported Xcode configuration for native preparation: ${CONFIGURATION:-<unset>}" >&2
        exit 1
        ;;
esac

cd "$REPOSITORY"
just ios-ffi-build "$platform" "$profile"
