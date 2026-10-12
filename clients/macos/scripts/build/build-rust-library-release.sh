#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/../apple/scripts/apple-platform.sh"
kodosi_apple_platform macosx

source "$ROOT/scripts/build/kodosi-paths.sh"
RUST_DIR=$(kodosi_runtime_root "$ROOT")
VALIDATION_MODE=${1:-release}
if [[ "$VALIDATION_MODE" != release && "$VALIDATION_MODE" != working-tree-validation ]]; then
    echo "Unknown Rust release build mode: $VALIDATION_MODE" >&2
    exit 1
fi
cd "$ROOT"
if [[ "$VALIDATION_MODE" == release ]]; then
    "$ROOT/scripts/build/verify-kodosi-checkout.sh"
fi
working_tree=
if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
    working_tree=--working-tree
fi
python3 "$RUST_DIR/../terminal/ghostty/scripts/verify-ghostty-pins.py" ${working_tree:+"$working_tree"}
python3 "$RUST_DIR/../scripts/build-runtime-library.py" \
    --manifest "$RUST_DIR/Cargo.toml" --output-dir "$ROOT/build/rust-ffi" \
    --profile release --target aarch64-apple-darwin \
    --consume-command bash "$ROOT/scripts/build/install-rust-artifact.sh" macos-arm64-release
