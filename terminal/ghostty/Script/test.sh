#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-/tmp/clang-module-cache}"
export SWIFTPM_MODULECACHE_OVERRIDE="${SWIFTPM_MODULECACHE_OVERRIDE:-/tmp/swiftpm-module-cache}"

./Script/verify-platform-pins.py
./Script/test-platform-pins.py
./Script/verify-third-party-notices.py
./Script/test-z2d-source-acquisition.py

echo "[*] build target=GhosttyTerminal platform=macOS"
swift build --target GhosttyTerminal --disable-automatic-resolution

echo "[*] package build and provenance checks passed"
