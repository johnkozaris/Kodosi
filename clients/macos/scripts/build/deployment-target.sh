#!/usr/bin/env bash

kodosi_load_macos_deployment_target() {
    local root=$1
    local authority=${KODOSI_MACOS_DEPLOYMENT_TARGET_FILE:-"$root/Config/macOSDeploymentTarget"}
    local value
    if [[ ! -f "$authority" ]]; then
        echo "macOS deployment target authority is missing: $authority" >&2
        return 1
    fi
    value=$(<"$authority")
    if [[ ! "$value" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]]; then
        echo "Invalid macOS deployment target in $authority: $value" >&2
        return 1
    fi
    export KODOSI_MACOS_DEPLOYMENT_TARGET=$value
    export MACOSX_DEPLOYMENT_TARGET=$value
}
