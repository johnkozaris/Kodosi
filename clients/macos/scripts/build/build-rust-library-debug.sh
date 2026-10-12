#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"

source "$ROOT/scripts/build/kodosi-paths.sh"
RUST_DIR=$(kodosi_runtime_root "$ROOT")
cd "$ROOT"
working_tree=
if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
    working_tree=--working-tree
fi
python3 "$RUST_DIR/../terminal/ghostty/scripts/verify-ghostty-pins.py" ${working_tree:+"$working_tree"}
target=$(cd "$RUST_DIR" && rustc -vV | grep '^host:' | cut -d' ' -f2)
python3 "$RUST_DIR/../scripts/build-runtime-library.py" \
    --manifest "$RUST_DIR/Cargo.toml" --output-dir "$ROOT/build/rust-ffi" \
    --profile dev --target "$target" \
    --consume-command bash "$ROOT/scripts/build/install-rust-artifact.sh" macos-host-debug "$(uname -m)"
