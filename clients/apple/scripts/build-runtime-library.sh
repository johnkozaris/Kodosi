#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <macosx|iphoneos|iphonesimulator> <Debug|Release|WorkingTreeValidation> <destination-folder>" >&2
    exit 64
fi

scripts=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$scripts/apple-platform.sh"
kodosi_apple_platform "$1"
repository=$(kodosi_repository_root)
case "$2" in
    Debug) profile=dev ;;
    Release | WorkingTreeValidation) profile=release ;;
    *)
        echo "Unknown Xcode configuration: $2. Use Debug, Release or WorkingTreeValidation." >&2
        exit 64
        ;;
esac
if [[ $1 == macosx && $2 == Release ]]; then
    "$scripts/verify-kodosi-checkout.sh"
fi

working_tree=
if [[ ${KODOSI_WORKING_TREE_VALIDATION:-0} == 1 ]]; then
    working_tree=--working-tree
fi
python3 "$repository/terminal/ghostty/scripts/verify-ghostty-pins.py" ${working_tree:+"$working_tree"}

viewer=
if [[ $1 != macosx ]]; then
    viewer=--no-default-features
    # Cargo also builds programs for the Mac, for example the build scripts. They must not use the iOS SDK.
    unset SDKROOT
fi
# Xcode gives its build phases a PATH without the Rust tools.
export PATH="$HOME/.cargo/bin:$PATH"
folder=$(python3 "$repository/scripts/build-runtime-library.py" \
    --manifest "$repository/runtime/Cargo.toml" \
    --output-dir "$repository/runtime/target/runtime-library" \
    --target "$KODOSI_RUST_TARGET" \
    --profile "$profile" \
    ${viewer:+"$viewer"})
"$scripts/verify-runtime-library.sh" "$1" "$folder"
mkdir -p "$3"
if ! cmp -s "$folder/libkodosi_ffi_c.a" "$3/libkodosi_runtime.a"; then
    cp "$folder/libkodosi_ffi_c.a" "$3/libkodosi_runtime.a"
fi
