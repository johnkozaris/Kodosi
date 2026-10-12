#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
variant=${1:?Usage: install-rust-artifact.sh <variant> <bundle>}
bundle=${2:?}
case "$variant" in
    macos-host-debug|macos-arm64-release) ;;
    *) echo "Unsupported Rust artifact variant: $variant" >&2; exit 1 ;;
esac
"$ROOT/../apple/scripts/verify-runtime-library.sh" macosx "$bundle" >&2
destination="$ROOT/Frameworks/KodosiKit.xcframework/$variant"
mkdir -p "$destination"
temporary_archive="$destination/libkodosi_runtime.a.tmp.$$"
trap 'rm -f "$temporary_archive"' EXIT
cp "$bundle/libkodosi_ffi_c.a" "$temporary_archive"
mv -f "$temporary_archive" "$destination/libkodosi_runtime.a"
