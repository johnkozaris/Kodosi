#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-/tmp/clang-module-cache}"
export SWIFTPM_MODULECACHE_OVERRIDE="${SWIFTPM_MODULECACHE_OVERRIDE:-/tmp/swiftpm-module-cache}"

./scripts/verify-platform-pins.py
./scripts/tests/test_platform_pins.py
./scripts/verify-third-party-notices.py
./scripts/tests/test_z2d_source_acquisition.py

echo "[*] build target=GhosttyTerminal platform=macOS"
swift build --target GhosttyTerminal --disable-automatic-resolution

for destination in 'generic/platform=iOS Simulator' 'generic/platform=iOS'; do
    echo "[*] build scheme=KodosiGhostty destination=$destination"
    xcodebuild \
        -quiet \
        -scheme KodosiGhostty \
        -destination "$destination" \
        -derivedDataPath .build/xcode \
        build
done

echo "[*] package build and provenance checks passed"
