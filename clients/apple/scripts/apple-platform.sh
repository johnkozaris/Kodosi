#!/usr/bin/env bash

kodosi_load_macos_deployment_target() {
    local value
    value=$(<"$1/Config/macOSDeploymentTarget")
    export KODOSI_MACOS_DEPLOYMENT_TARGET=$value
    export MACOSX_DEPLOYMENT_TARGET=$value
}
