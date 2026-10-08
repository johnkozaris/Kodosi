#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
variant=${1:?Usage: install-rust-artifact.sh <variant> <architecture> <bundle>}
architecture=${2:?}
bundle=${3:?}
case "$variant" in
    macos-host-debug|macos-arm64-release) ;;
    *) echo "Unsupported Rust artifact variant: $variant" >&2; exit 1 ;;
esac
source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
"$ROOT/scripts/build/verify-rust-archive.sh" "$bundle" "$KODOSI_MACOS_DEPLOYMENT_TARGET" "$architecture" >&2
destination="$ROOT/Frameworks/KodosiKit.xcframework/$variant"
mkdir -p "$destination"
temporary_archive="$destination/libkodosi_runtime.a.tmp.$$"
trap 'rm -f "$temporary_archive"' EXIT
cp "$bundle/libkodosi_ffi_c.a" "$temporary_archive"
mv -f "$temporary_archive" "$destination/libkodosi_runtime.a"
