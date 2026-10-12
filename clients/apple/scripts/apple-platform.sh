#!/usr/bin/env bash

kodosi_repository_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
}

kodosi_apple_platform() {
    local record
    case "${1:-}" in
        macosx) record=clients/macos/Config/macOSDeploymentTarget ;;
        iphoneos | iphonesimulator) record=clients/apple/Config/iOSDeploymentTarget ;;
        *)
            echo "Unknown Apple platform: ${1:-<none>}. Use macosx, iphoneos or iphonesimulator." >&2
            return 1
            ;;
    esac
    KODOSI_DEPLOYMENT_TARGET=$(<"$(kodosi_repository_root)/$record")
    export KODOSI_DEPLOYMENT_TARGET
    if [[ $1 == macosx ]]; then
        export MACOSX_DEPLOYMENT_TARGET=$KODOSI_DEPLOYMENT_TARGET
    else
        export IPHONEOS_DEPLOYMENT_TARGET=$KODOSI_DEPLOYMENT_TARGET
    fi
}
