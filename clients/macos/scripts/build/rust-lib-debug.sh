#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"

source "$ROOT/scripts/build/kodosi-paths.sh"
RUST_DIR=$(kodosi_runtime_root "$ROOT")
cd "$ROOT"
"$ROOT/scripts/build/verify-ghostty-checkout.sh"
target=$(cd "$RUST_DIR" && rustc -vV | grep '^host:' | cut -d' ' -f2)
python3 "$RUST_DIR/../scripts/build-ffi-artifact.py" \
    --manifest "$RUST_DIR/Cargo.toml" --output-dir "$ROOT/build/rust-ffi" \
    --profile dev --target "$target" \
    --consume-command bash "$ROOT/scripts/build/install-rust-artifact.sh" macos-host-debug "$(uname -m)"
