#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
    echo "usage: $0 <ffi-artifact-bundle> <maximum-deployment-target> <expected-architecture> [...]" >&2
    exit 64
fi

bundle=$1
archive="$bundle/libkodosi_ffi_c.a"
header="$bundle/include/kodosi_runtime.h"
maximum_deployment_target=$2
shift 2

if [[ ! -f "$archive" ]]; then
    echo "Rust archive is missing: $archive" >&2
    exit 1
fi

actual_architectures=$(lipo -archs "$archive")
for expected_architecture in "$@"; do
    if [[ " $actual_architectures " != *" $expected_architecture "* ]]; then
        echo "Rust archive $archive is missing $expected_architecture (has: $actual_architectures)" >&2
        exit 1
    fi
done

if [[ $(wc -w <<<"$actual_architectures") -ne $# ]]; then
    echo "Rust archive $archive has unexpected architectures: $actual_architectures" >&2
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

required_vt_imports=(
    _ghostty_terminal_new
    _ghostty_terminal_vt_write
    _ghostty_terminal_resize
    _ghostty_checkpoint_encode_buf
    _ghostty_checkpoint_restore
)

script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
swift_root=$(cd "$script_directory/../.." && pwd)

source "$script_directory/kodosi-paths.sh"
runtime_directory=$(kodosi_runtime_root "$swift_root")
python3 - "$bundle" <<'PYVERIFY'
import hashlib
import json
import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve(strict=True)
metadata = json.loads((root / "artifact.json").read_bytes())
if set(metadata) != {"schemaVersion", "target", "profile", "archiveSha256", "headerSha256"} or metadata["schemaVersion"] != 1:
    raise SystemExit("Invalid FFI artifact bundle metadata")
for name, key in (("libkodosi_ffi_c.a", "archiveSha256"), ("include/kodosi_runtime.h", "headerSha256")):
    path = root / name
    if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != metadata[key]:
        raise SystemExit(f"FFI artifact bundle association mismatch: {name}")
PYVERIFY
"$script_directory/header-parity.sh" "$bundle"
expected_c_symbols=$(
    perl -0777 -ne 'while (/\b(kodosi_[A-Za-z0-9_]+)\s*\(/g) { print "_$1\n" }' "$header" |
        sort -u
)
if [[ -z "$expected_c_symbols" ]]; then
    echo "Generated Rust header declares no kodosi_* functions: $header" >&2
    exit 1
fi

if [[ -n "${KODOSI_RUST_LLVM_NM:-}" ]]; then
    rust_llvm_nm=$KODOSI_RUST_LLVM_NM
elif [[ -d "$runtime_directory" ]]; then
    rust_sysroot=$(cd "$runtime_directory" && rustc --print sysroot)
    rust_host=$(cd "$runtime_directory" && rustc -vV | grep '^host:' | cut -d' ' -f2)
    rust_llvm_nm="$rust_sysroot/lib/rustlib/$rust_host/bin/llvm-nm"
else
    rust_llvm_nm="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | grep '^host:' | cut -d' ' -f2)/bin/llvm-nm"
fi
if [[ ! -x "$rust_llvm_nm" ]]; then
    echo "Rust LLVM symbol inspector is missing: $rust_llvm_nm" >&2
    echo "Install llvm-tools for the Rust toolchain selected by $runtime_directory" >&2
    exit 1
fi
rust_llvm_objdump="$(dirname "$rust_llvm_nm")/llvm-objdump"
if [[ ! -x "$rust_llvm_objdump" ]]; then
    echo "Rust LLVM disassembler is missing: $rust_llvm_objdump" >&2
    echo "Install llvm-tools for the Rust toolchain selected by $runtime_directory" >&2
    exit 1
fi

for architecture in "$@"; do
    thin_archive=
    if [[ $(wc -w <<<"$actual_architectures") -eq 1 ]]; then
        inspection_archive=$archive
        symbols=$("$rust_llvm_nm" --defined-only --extern-only "$archive")
        undefined_symbols=$("$rust_llvm_nm" --undefined-only "$archive" 2>/dev/null)
    else
        thin_archive=$(mktemp "${TMPDIR:-/tmp}/kodosi-runtime-${architecture}.XXXXXX.a")
        trap '[[ -n "${thin_archive:-}" ]] && rm -f "$thin_archive"' EXIT
        lipo "$archive" -thin "$architecture" -output "$thin_archive"
        inspection_archive=$thin_archive
        symbols=$("$rust_llvm_nm" --defined-only --extern-only "$inspection_archive")
        undefined_symbols=$("$rust_llvm_nm" --undefined-only "$inspection_archive" 2>/dev/null)
    fi
    actual_c_symbols=$(awk '{print $NF}' <<<"$symbols" | grep '^_kodosi_' | sort -u || true)
    if grep -Eq '[[:space:]]_ghostty_[[:alnum:]_]+$' <<<"$symbols"; then
        echo "Rust archive $archive bundles Ghostty definitions in its $architecture slice; the KodosiDesktop executable process must receive exactly one locked renderer/VT image at final link" >&2
        exit 1
    fi
    for symbol in "${required_vt_imports[@]}"; do
        if ! grep -Eq "(^|[[:space:]])${symbol}$" <<<"$undefined_symbols"; then
            echo "Rust archive $archive lacks required libghostty-vt authority import $symbol in its $architecture slice" >&2
            exit 1
        fi
    done
    if ! diff -u <(printf '%s\n' "$expected_c_symbols") <(printf '%s\n' "$actual_c_symbols") >/dev/null; then
        echo "Rust archive $archive public kodosi_* exports differ from $header in its $architecture slice" >&2
        diff -u <(printf '%s\n' "$expected_c_symbols") <(printf '%s\n' "$actual_c_symbols") >&2 || true
        exit 1
    fi
    protocol_disassembly=$("$rust_llvm_objdump" \
        --disassemble-symbols=_kodosi_protocol_version \
        "$inspection_archive" 2>/dev/null)
    case "$architecture" in
        arm64)
            protocol_constant_pattern='mov[[:space:]]+w0,[[:space:]]+#(0x35|53)([[:space:];]|$)'
            ;;
        x86_64)
            protocol_constant_pattern='movl?[[:space:]]+\$(0x35|53),[[:space:]]+%eax([[:space:];]|$)'
            ;;
        *)
            echo "Rust archive verifier cannot inspect protocol constants for $architecture" >&2
            exit 1
            ;;
    esac
    if ! grep -Eq "$protocol_constant_pattern" <<<"$protocol_disassembly"; then
        echo "Rust archive $archive does not return exact desktop/runtime protocol v53 in its $architecture slice" >&2
        exit 1
    fi
    abi_disassembly=$("$rust_llvm_objdump" \
        --disassemble-symbols=_kodosi_abi_version \
        "$inspection_archive" 2>/dev/null)
    case "$architecture" in
        arm64) abi_constant_pattern='mov[[:space:]]+w0,[[:space:]]+#(0x7|7)([[:space:];]|$)' ;;
        x86_64) abi_constant_pattern='movl?[[:space:]]+\$(0x7|7),[[:space:]]+%eax([[:space:];]|$)' ;;
    esac
    if ! grep -Eq "$abi_constant_pattern" <<<"$abi_disassembly"; then
        echo "Rust archive $archive does not return exact C ABI v7 in its $architecture slice" >&2
        exit 1
    fi
    if [[ -n "$thin_archive" ]]; then
        rm -f "$thin_archive"
        thin_archive=
        trap - EXIT
    fi
done

echo "Verified Rust archive: $archive ($actual_architectures, macOS <= $maximum_deployment_target)"
