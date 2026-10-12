#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
python3 "$ROOT/scripts/build/generate-xcode-project.py" "$ROOT"
