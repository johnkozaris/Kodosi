#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/../apple/scripts/apple-platform.sh"
kodosi_apple_platform macosx
KODOSI_VERSION=$(<"$ROOT/../../VERSION") KODOSI_MACOS_DEPLOYMENT_TARGET=$KODOSI_DEPLOYMENT_TARGET python3 "$ROOT/scripts/build/generate-xcode-project.py" "$ROOT"
