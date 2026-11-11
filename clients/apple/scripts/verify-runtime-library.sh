#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <macosx|iphoneos|iphonesimulator> <runtime-library-folder>" >&2
    exit 64
fi

scripts=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$scripts/apple-platform.sh"
kodosi_apple_platform "$1"
archive="$2/libkodosi_ffi_c.a"

# The C code of a Rust crate takes the version of the SDK when the build does not give it the
# deployment target, and then the app does not start on the older system that it names.
later=$(kodosi_later_deployment_targets "$archive")
if [[ -n "$later" ]]; then
    echo "The $1 runtime library $archive needs $1 $(xargs <<<"$later"). The deployment target is $KODOSI_DEPLOYMENT_TARGET." >&2
    exit 1
fi
