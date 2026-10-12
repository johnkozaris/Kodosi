#!/usr/bin/env bash

kodosi_repository_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
}

kodosi_apple_platform() {
    local record
    case "${1:-}" in
        macosx)
            KODOSI_RUST_TARGET=aarch64-apple-darwin
            record=clients/macos/Config/macOSDeploymentTarget
            ;;
        iphoneos)
            KODOSI_RUST_TARGET=aarch64-apple-ios
            record=clients/apple/Config/iOSDeploymentTarget
            ;;
        iphonesimulator)
            KODOSI_RUST_TARGET=aarch64-apple-ios-sim
            record=clients/apple/Config/iOSDeploymentTarget
            ;;
        *)
            echo "Unknown Apple platform: ${1:-<none>}. Use macosx, iphoneos or iphonesimulator." >&2
            return 1
            ;;
    esac
    KODOSI_DEPLOYMENT_TARGET=$(<"$(kodosi_repository_root)/$record")
    export KODOSI_RUST_TARGET KODOSI_DEPLOYMENT_TARGET
    if [[ $1 == macosx ]]; then
        export MACOSX_DEPLOYMENT_TARGET=$KODOSI_DEPLOYMENT_TARGET
    else
        export IPHONEOS_DEPLOYMENT_TARGET=$KODOSI_DEPLOYMENT_TARGET
    fi
}

kodosi_later_deployment_targets() {
    local versions
    versions=$(otool -l "$1" | awk '
        $1 == "cmd" { build = ($2 == "LC_BUILD_VERSION"); legacy = ($2 ~ /^LC_VERSION_MIN_/); next }
        (build && $1 == "minos") || (legacy && $1 == "version") { print $2; build = 0; legacy = 0 }
    ')
    if [[ -z "$versions" ]]; then
        echo "$1 has no deployment target." >&2
        return 1
    fi
    awk -v maximum="$KODOSI_DEPLOYMENT_TARGET" '
        function code(version, parts) {
            split(version, parts, ".")
            return (parts[1] + 0) * 1000000 + (parts[2] + 0) * 1000 + (parts[3] + 0)
        }
        code($1) > code(maximum) { print $1 }
    ' <<<"$versions" | sort -u
}
