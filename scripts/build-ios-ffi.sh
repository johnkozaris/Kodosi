#!/usr/bin/env bash
set -euo pipefail

usage="Usage: build-ios-ffi.sh <iphoneos|iphonesimulator> <dev|release>"
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
sdk=${1:?$usage}
profile=${2:?$usage}
case "$sdk" in
    iphoneos) target=aarch64-apple-ios ;;
    iphonesimulator) target=aarch64-apple-ios-sim ;;
    *) echo "$usage" >&2; exit 64 ;;
esac
case "$profile" in
    dev | release) ;;
    *) echo "$usage" >&2; exit 64 ;;
esac

IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-$(cat "$root/clients/apple/Config/iOSDeploymentTarget")}
export IPHONEOS_DEPLOYMENT_TARGET
unset SDKROOT

bundle=$(python3 "$root/scripts/build-runtime-library.py" \
    --manifest "$root/runtime/Cargo.toml" --output-dir "$root/runtime/target/ffi-artifacts" \
    --profile "$profile" --target "$target" --no-default-features)

header="$root/clients/apple/Sources/KodosiKit/kodosi_runtime.h"
if ! cmp -s "$bundle/include/kodosi_runtime.h" "$header"; then
    diff -u "$header" "$bundle/include/kodosi_runtime.h" >&2 || true
    echo "The iOS runtime header differs from $header; the C ABI must be the same on every platform." >&2
    exit 1
fi

library="$root/clients/ios/Frameworks/Runtime/$sdk/libkodosi_runtime.a"
mkdir -p "$(dirname "$library")"
if ! cmp -s "$bundle/libkodosi_ffi_c.a" "$library"; then
    temporary="$library.tmp.$$"
    trap 'rm -f "$temporary"' EXIT
    cp "$bundle/libkodosi_ffi_c.a" "$temporary"
    mv -f "$temporary" "$library"
fi
printf '%s\n' "$library"
