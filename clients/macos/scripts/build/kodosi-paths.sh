#!/usr/bin/env bash

kodosi_repository_root() {
    local swift_root=$1
    local repository="$swift_root/../.."
    (cd "$repository" && pwd)
}

kodosi_runtime_root() {
    local swift_root=$1
    printf '%s/runtime\n' "$(kodosi_repository_root "$swift_root")"
}
