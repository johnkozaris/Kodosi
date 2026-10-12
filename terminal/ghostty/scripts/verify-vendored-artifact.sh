#!/bin/bash

set -euo pipefail

usage() {
    echo "Usage: $0 [--vendor <folder>]" >&2
}

VENDOR_ROOT=
while [ $# -gt 0 ]; do
    case "$1" in
        --vendor)
            if [ $# -lt 2 ]; then
                usage
                exit 1
            fi
            VENDOR_ROOT=$(cd "$2" && pwd)
            shift 2
            ;;
        *)
            usage
            exit 1
            ;;
    esac
done

cd "$(dirname "$0")/.."

VENDOR_ROOT=${VENDOR_ROOT:-Vendor}
RENDERER="$VENDOR_ROOT/GhosttyKit.xcframework"
VT="$VENDOR_ROOT/GhosttyVt/macos-arm64"
MACOS_MINIMUM=13.0
APPLE_SLICES=$(./scripts/native-build-options.sh slices)
SLICES=()
IOS_SLICES=()
while IFS= read -r slice; do
    SLICES+=("$slice")
    [ "$slice" = macos-arm64 ] || IOS_SLICES+=("$slice")
done <<<"$APPLE_SLICES"

ios_slice_values() {
    local target
    target=$(./scripts/native-build-options.sh target "$1")
    if [[ "$target" != aarch64-ios.* ]]; then
        echo "[!] unexpected iOS target for $1: $target"
        exit 1
    fi
    SLICE_MINIMUM=${target#aarch64-ios.}
    SLICE_MINIMUM=${SLICE_MINIMUM%-simulator}
    case "$1" in
        *-simulator)
            SLICE_PLATFORM=7
            SLICE_SDK=iphonesimulator
            SLICE_CLANG_TARGET="arm64-apple-ios$SLICE_MINIMUM-simulator"
            ;;
        *)
            SLICE_PLATFORM=2
            SLICE_SDK=iphoneos
            SLICE_CLANG_TARGET="arm64-apple-ios$SLICE_MINIMUM"
            ;;
    esac
}

require_build_version() {
    local archive=$1
    local platform=$2
    local minimum=$3
    local members
    local versions
    members=$(ar -t "$archive" | grep -vc '^__\.SYMDEF' || true)
    versions=$(otool -l "$archive" | awk '
        /cmd LC_BUILD_VERSION/ { build = 1; next }
        /cmd LC_VERSION_MIN/ { print "legacy"; next }
        build && $1 == "platform" { platform = $2 }
        build && $1 == "minos" { print platform, $2; build = 0 }
    ')
    if [ "$(printf '%s\n' "$versions" | grep -c . || true)" != "$members" ] ||
        printf '%s\n' "$versions" | grep -vxqF "$platform $minimum"; then
        echo "[!] Ghostty archive objects do not all target platform $platform minimum $minimum: $archive"
        exit 1
    fi
}

./scripts/verify-xcframework.sh "$RENDERER"
RENDERER_ARCHIVE="$RENDERER/macos-arm64/libghostty.a"
VT_ARCHIVE="$VT/lib/libghostty-vt.a"
ARM64_ARCHIVES=("$VT_ARCHIVE")
for slice in "${SLICES[@]}"; do
    ARM64_ARCHIVES+=("$RENDERER/$slice/libghostty.a")
done
for archive in "${ARM64_ARCHIVES[@]}"; do
    if [ "$(lipo -archs "$archive")" != arm64 ]; then
        echo "[!] Ghostty archive is not arm64-only: $archive"
        exit 1
    fi
done
require_build_version "$RENDERER_ARCHIVE" 1 "$MACOS_MINIMUM"
require_build_version "$VT_ARCHIVE" 1 "$MACOS_MINIMUM"
for slice in "${IOS_SLICES[@]}"; do
    ios_slice_values "$slice"
    require_build_version "$RENDERER/$slice/libghostty.a" "$SLICE_PLATFORM" "$SLICE_MINIMUM"
done
for slice in "${IOS_SLICES[@]}"; do
    for header in ghostty.h module.modulemap; do
        if ! cmp -s "$RENDERER/macos-arm64/Headers/$header" "$RENDERER/$slice/Headers/$header"; then
            echo "[!] $slice renderer header differs from macOS: $header"
            exit 1
        fi
    done
done

if [ ! -f "$VT/include/ghostty/vt.h" ] ||
    [ ! -f "$VT/include/module.modulemap" ] ||
    ! grep -Fq 'module GhosttyVt' "$VT/include/module.modulemap"; then
    echo "[!] VT headers or module map are incomplete"
    exit 1
fi

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-verify-vendored-artifact.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT

for slice in "${SLICES[@]}"; do
python3 - \
    "$RENDERER/$slice/Headers/ghostty.h" \
    "$VT/include/ghostty" \
    "$RENDERER/$slice/libghostty.a" \
    "$TMP_DIR" \
    "$slice" <<'PY'
import re
import subprocess
import sys
from pathlib import Path

renderer_header = Path(sys.argv[1])
vt_headers = Path(sys.argv[2])
archive = Path(sys.argv[3])
out_dir = Path(sys.argv[4])
slice_name = sys.argv[5]

def declared(paths):
    text = "\n".join(path.read_text() for path in paths)
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    text = re.sub(r"//.*", "", text)
    return {
        match
        for match in re.findall(
            r"\bGHOSTTY_API\b[^;{}]*?\b(ghostty_[A-Za-z0-9_]+)\s*\(",
            text,
            flags=re.S,
        )
    }

renderer = declared([renderer_header])
vt = {
    name
    for name in declared(vt_headers.rglob("*.h"))
    if not name.startswith("ghostty_wasm_")
}
output = subprocess.check_output(["nm", "-gU", str(archive)], text=True)
strong = []
for line in output.splitlines():
    match = re.search(r"\b[A-Z]\s+_([A-Za-z0-9_$.]+)$", line)
    if match:
        strong.append(match.group(1))
duplicates = sorted(name for name in set(strong) if strong.count(name) > 1)
if duplicates:
    print(f"[!] {slice_name} combined archive defines duplicate strong symbols:")
    print("\n".join(duplicates))
    raise SystemExit(1)
exported = {name for name in strong if name.startswith("ghostty_")}
missing_renderer = sorted(renderer - exported)
missing_vt = sorted(vt - exported)
internal_exports = {
    "ghostty_hwy_detect_targets",
    "ghostty_simd_base64_decode",
    "ghostty_simd_base64_max_length",
    "ghostty_simd_codepoint_width",
    "ghostty_simd_decode_utf8_until_control_seq",
    "ghostty_simd_index_of",
}
undeclared = sorted(exported - renderer - vt - internal_exports)
if missing_renderer or missing_vt or undeclared:
    if missing_renderer:
        print(f"[!] {slice_name} combined archive is missing renderer C APIs:")
        print("\n".join(missing_renderer))
    if missing_vt:
        print(f"[!] {slice_name} combined archive is missing VT C APIs:")
        print("\n".join(missing_vt))
    if undeclared:
        print(f"[!] {slice_name} combined archive exports undeclared Ghostty APIs:")
        print("\n".join(undeclared))
    raise SystemExit(1)

(out_dir / f"{slice_name}-renderer-api.txt").write_text("\n".join(sorted(renderer)) + "\n")
(out_dir / f"{slice_name}-vt-api.txt").write_text("\n".join(sorted(vt)) + "\n")
PY
done

cat > "$TMP_DIR/renderer.c" <<'C'
#include "ghostty.h"

int renderer_probe(int argc, char **argv) {
    ghostty_info_s info = ghostty_info();
    if (info.build_mode != GHOSTTY_BUILD_MODE_RELEASE_FAST) return 1;

    ghostty_result_e (*restore_checkpoint)(
        ghostty_surface_t,
        const uint8_t *,
        size_t,
        const ghostty_checkpoint_restore_options_s *,
        ghostty_checkpoint_info_s *) = ghostty_surface_checkpoint_restore;
    ghostty_clipboard_request_e (*request_type)(const void *) =
        ghostty_clipboard_request_type;
    (void)restore_checkpoint;
    (void)request_type;

    return ghostty_init((uintptr_t)argc, argv);
}
C
cat > "$TMP_DIR/vt.c" <<'C'
#define GHOSTTY_STATIC
#include <ghostty/vt.h>

int vt_probe(void) {
    GhosttyOptimizeMode optimize = GHOSTTY_OPTIMIZE_DEBUG;
    if (ghostty_build_info(GHOSTTY_BUILD_INFO_OPTIMIZE, &optimize) != GHOSTTY_SUCCESS) return 2;
    if (optimize != GHOSTTY_OPTIMIZE_RELEASE_FAST) return 3;

    bool tmux_control_mode = true;
    if (ghostty_build_info(GHOSTTY_BUILD_INFO_TMUX_CONTROL_MODE, &tmux_control_mode) != GHOSTTY_SUCCESS) return 4;
    if (tmux_control_mode) return 5;

    GhosttyTerminal terminal = NULL;
    if (ghostty_terminal_new(NULL, &terminal, 80, 24) != GHOSTTY_SUCCESS) return 6;
    ghostty_terminal_vt_write(terminal, (const uint8_t *)"ok", 2);
    ghostty_terminal_free(terminal);
    return 0;
}
C
cat > "$TMP_DIR/main.c" <<'C'
int renderer_probe(int, char **);
int vt_probe(void);

int main(int argc, char **argv) {
    int result = renderer_probe(argc, argv);
    return result != 0 ? result : vt_probe();
}
C

xcrun clang \
    -arch arm64 \
    -mmacosx-version-min="$MACOS_MINIMUM" \
    -I"$RENDERER/macos-arm64/Headers" \
    -c "$TMP_DIR/renderer.c" \
    -o "$TMP_DIR/renderer.o"
xcrun clang \
    -arch arm64 \
    -mmacosx-version-min="$MACOS_MINIMUM" \
    -I"$VT/include" \
    -c "$TMP_DIR/vt.c" \
    -o "$TMP_DIR/vt.o"
xcrun clang \
    -arch arm64 \
    -mmacosx-version-min="$MACOS_MINIMUM" \
    -c "$TMP_DIR/main.c" \
    -o "$TMP_DIR/main.o"
xcrun clang \
    "$TMP_DIR/renderer.o" \
    "$TMP_DIR/vt.o" \
    "$TMP_DIR/main.o" \
    "$RENDERER_ARCHIVE" \
    "$VT_ARCHIVE" \
    -lc++ \
    -framework Foundation \
    -framework CoreFoundation \
    -framework CoreGraphics \
    -framework CoreText \
    -framework CoreVideo \
    -framework QuartzCore \
    -framework IOSurface \
    -framework Carbon \
    -framework Metal \
    -o "$TMP_DIR/combined-link-probe"
"$TMP_DIR/combined-link-probe"

ios_link_probe() {
    local slice=$1
    local sdk=$2
    local target=$3
    local probe_dir="$TMP_DIR/$slice"
    mkdir -p "$probe_dir"
    xcrun --sdk "$sdk" clang \
        -target "$target" \
        -I"$RENDERER/$slice/Headers" \
        -c "$TMP_DIR/renderer.c" \
        -o "$probe_dir/renderer.o"
    xcrun --sdk "$sdk" clang \
        -target "$target" \
        -I"$VT/include" \
        -c "$TMP_DIR/vt.c" \
        -o "$probe_dir/vt.o"
    xcrun --sdk "$sdk" clang \
        -target "$target" \
        -c "$TMP_DIR/main.c" \
        -o "$probe_dir/main.o"
    xcrun --sdk "$sdk" clang \
        -target "$target" \
        "$probe_dir/renderer.o" \
        "$probe_dir/vt.o" \
        "$probe_dir/main.o" \
        "$RENDERER/$slice/libghostty.a" \
        -lc++ \
        -framework CoreFoundation \
        -framework CoreGraphics \
        -framework CoreText \
        -framework QuartzCore \
        -framework IOSurface \
        -framework Metal \
        -o "$probe_dir/combined-link-probe"
}
for slice in "${IOS_SLICES[@]}"; do
    ios_slice_values "$slice"
    ios_link_probe "$slice" "$SLICE_SDK" "$SLICE_CLANG_TARGET"
done

for slice in "${SLICES[@]}"; do
    archive="$RENDERER/$slice/libghostty.a"
    if nm -u "$archive" | grep -q '__libcpp_verbose_abort'; then
        echo "[!] obsolete libc++ verbose-abort dependency remains in $slice"
        exit 1
    fi
    if strings "$archive" | grep -q 'CAIOSurfaceLayer'; then
        echo "[!] private CAIOSurfaceLayer remains in $slice renderer artifact"
        exit 1
    fi
done

echo "[*] vendored renderer and VT paths contain one verified native image per Apple slice"
