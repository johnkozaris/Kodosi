#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <ffi-artifact-bundle> <maximum-deployment-target> <architecture>" >&2
    exit 64
fi

bundle=$1
archive="$bundle/libkodosi_ffi_c.a"
header="$bundle/include/kodosi_runtime.h"
maximum_deployment_target=$2
architecture=$3

if [[ ! -f "$archive" ]]; then
    echo "Rust archive is missing: $archive" >&2
    exit 1
fi

actual_architectures=$(lipo -archs "$archive")
if [[ "$actual_architectures" != "$architecture" ]]; then
    echo "Rust archive $archive has architectures $actual_architectures, expected $architecture" >&2
    exit 1
fi

deployment_versions=$(
    otool -l "$archive" | awk '
        $1 == "cmd" {
            build_version = ($2 == "LC_BUILD_VERSION")
            legacy_version = ($2 == "LC_VERSION_MIN_MACOSX")
            next
        }
        build_version && $1 == "minos" {
            print $2
            build_version = 0
            next
        }
        legacy_version && $1 == "version" {
            print $2
            legacy_version = 0
        }
    '
)
if [[ -z "$deployment_versions" ]]; then
    echo "Rust archive $archive has no macOS deployment metadata" >&2
    exit 1
fi

newer_deployment_versions=$(
    awk -v maximum="$maximum_deployment_target" '
        function version_code(version, parts) {
            split(version, parts, ".")
            return (parts[1] + 0) * 1000000 + (parts[2] + 0) * 1000 + (parts[3] + 0)
        }
        version_code($1) > version_code(maximum) {
            print $1
        }
    ' <<<"$deployment_versions" | sort -u
)
if [[ -n "$newer_deployment_versions" ]]; then
    echo "Rust archive $archive exceeds macOS $maximum_deployment_target (found: $(tr '\n' ' ' <<<"$newer_deployment_versions" | xargs))" >&2
    exit 1
fi

script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/kodosi-paths.sh"
runtime_directory=$(kodosi_runtime_root "$(cd "$script_directory/../.." && pwd)")
"$script_directory/verify-runtime-header.sh" "$bundle"
expected_c_symbols=$(
    perl -0777 -ne 'while (/\b(kodosi_[A-Za-z0-9_]+)\s*\(/g) { print "_$1\n" }' "$header" |
        sort -u
)
if [[ -z "$expected_c_symbols" ]]; then
    echo "Generated Rust header declares no kodosi_* functions: $header" >&2
    exit 1
fi

rust_sysroot=$(cd "$runtime_directory" && rustc --print sysroot)
rust_host=$(cd "$runtime_directory" && rustc -vV | grep '^host:' | cut -d' ' -f2)
rust_llvm_nm="$rust_sysroot/lib/rustlib/$rust_host/bin/llvm-nm"
if [[ ! -x "$rust_llvm_nm" ]]; then
    echo "Rust LLVM symbol inspector is missing: $rust_llvm_nm" >&2
    echo "Install llvm-tools for the Rust toolchain selected by $runtime_directory" >&2
    exit 1
fi

symbols=$("$rust_llvm_nm" --defined-only --extern-only --quiet "$archive")
actual_c_symbols=$(awk '{print $NF}' <<<"$symbols" | grep '^_kodosi_' | sort -u || true)
if grep -Eq '[[:space:]]_ghostty_[[:alnum:]_]+$' <<<"$symbols"; then
    echo "Rust archive $archive bundles Ghostty definitions; the KodosiDesktop executable process must receive exactly one locked renderer/VT image at final link" >&2
    exit 1
fi
if ! diff -u <(printf '%s\n' "$expected_c_symbols") <(printf '%s\n' "$actual_c_symbols") >&2; then
    echo "Rust archive $archive public kodosi_* exports differ from $header" >&2
    exit 1
fi

echo "Verified Rust archive: $archive ($architecture, macOS <= $maximum_deployment_target)"
