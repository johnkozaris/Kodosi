#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <macosx|iphoneos|iphonesimulator> <runtime-library-folder>" >&2
    exit 64
fi

scripts=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$scripts/apple-platform.sh"
kodosi_apple_platform "$1"
platform=$1
archive="$2/libkodosi_ffi_c.a"
header="$2/include/kodosi_runtime.h"
repository=$(kodosi_repository_root)
committed_header=clients/apple/Sources/KodosiKit/kodosi_runtime.h

fail() {
    echo "The $platform runtime library $archive $1" >&2
    exit 1
}

[[ -f "$archive" ]] || fail "does not exist."
if ! cmp -s "$header" "$repository/$committed_header"; then
    diff -u "$repository/$committed_header" "$header" >&2 || true
    fail "has another header than $committed_header. Run just protocol-gen, and do not edit the header by hand."
fi

architectures=$(lipo -archs "$archive")
[[ "$architectures" == arm64 ]] || fail "has the architectures $architectures. It must have only arm64."
later=$(kodosi_later_deployment_targets "$archive")
[[ -z "$later" ]] || fail "needs $platform $(xargs <<<"$later"). The deployment target is $KODOSI_DEPLOYMENT_TARGET."

llvm_nm="$(cd "$repository" && rustc --print sysroot)/lib/rustlib/$(cd "$repository" && rustc -vV | sed -n 's/^host: //p')/bin/llvm-nm"
[[ -x "$llvm_nm" ]] || fail "cannot be examined without $llvm_nm. Install llvm-tools for the Rust toolchain in rust-toolchain.toml."
symbols=$("$llvm_nm" --defined-only --extern-only --quiet "$archive" | awk 'NF { print $NF }')
if grep -q '^_ghostty_' <<<"$symbols"; then
    fail "contains Ghostty definitions. The app must link exactly one Ghostty image, the locked one."
fi
declared=$(perl -0777 -ne 'while (/\b(kodosi_[A-Za-z0-9_]+)\s*\(/g) { print "_$1\n" }' "$header" | sort -u)
exported=$(grep '^_kodosi_' <<<"$symbols" | sort -u || true)
if [[ -z "$declared" || "$declared" != "$exported" ]]; then
    diff -u <(printf '%s\n' "$declared") <(printf '%s\n' "$exported") >&2 || true
    fail "exports other kodosi_* functions than its header declares."
fi

echo "Verified the $platform runtime library $archive."
