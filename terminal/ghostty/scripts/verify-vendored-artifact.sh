#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

VENDOR_ROOT=${KODOSI_GHOSTTY_VENDOR_ROOT:-Vendor}
METADATA="$VENDOR_ROOT/libghostty.version"
RENDERER="$VENDOR_ROOT/GhosttyKit.xcframework"
VT="$VENDOR_ROOT/GhosttyVt/macos-arm64"
IOS_SLICES=(ios-arm64 ios-arm64-simulator)

if [ ! -f "$METADATA" ]; then
    echo "[!] metadata not found: $METADATA"
    exit 1
fi

ghostty_commit=
package_version=
patches_sha256=
patch_application_sha256=
zig_version=
zig_target=
zig_sha256=
zig_store_sha256=
metal_toolchain_build=
metal_toolchain_dmg_sha256=
combined_archive_sha256=
ghostty_build_options=
renderer_target=
renderer_cpu=
renderer_optimize=
renderer_artifact_sha256=
vt_target=
vt_cpu=
vt_optimize=
vt_artifact_sha256=
symbol_manifest_sha256=
ZIG_VERSION=
ZIG_TARGET=
ZIG_SHA256=
METAL_TOOLCHAIN_BUILD=
METAL_TOOLCHAIN_DMG_SHA256=

# shellcheck disable=SC1090
source "$METADATA"
# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source NativeBuild.env

EXPECTED_PACKAGE_VERSION=$(tr -d '[:space:]' < VERSION)
ACTUAL_RENDERER_DIGEST=$(./scripts/artifact-digest.sh "$RENDERER")
ACTUAL_VT_DIGEST=$(./scripts/artifact-digest.sh "$VT")
ACTUAL_SYMBOL_DIGEST=$(./scripts/artifact-digest.sh "$VENDOR_ROOT/symbols")

SOURCE_DIR=${KODOSI_GHOSTTY_PATCHED_SOURCE:-build/ghostty-source}
EXPECTED_GHOSTTY_COMMIT=$(tr -d '[:space:]' < MacOSGhostty.ref)
ACTUAL_PATCHES_DIGEST=$patches_sha256
ACTUAL_PATCH_APPLICATION_DIGEST=$patch_application_sha256
ACTUAL_ZIG_STORE_DIGEST=$zig_store_sha256
EXPECTED_BUILD_OPTIONS=$ghostty_build_options
if [ "${KODOSI_GHOSTTY_REQUIRE_BUILD_PROVENANCE:-0}" = 1 ]; then
    ACTUAL_PATCHES_DIGEST=$(./scripts/artifact-digest.sh Patches/ghostty)
    EXPECTED_BUILD_OPTIONS=$(./scripts/native-build-options.sh vector)
    if [ ! -d "$SOURCE_DIR/.git" ] || [ ! -d "$SOURCE_DIR/zig-pkg" ]; then
        echo "[!] patched Ghostty source and Zig store are required for strict provenance verification"
        exit 1
    fi
    ACTUAL_PATCH_APPLICATION_DIGEST=$(
        ./scripts/patch-application-digest.sh "$SOURCE_DIR" "$EXPECTED_GHOSTTY_COMMIT"
    )
    ACTUAL_ZIG_STORE_DIGEST=$(./scripts/tree-digest.sh "$SOURCE_DIR/zig-pkg")
fi

if [ "$ghostty_commit" != "$EXPECTED_GHOSTTY_COMMIT" ] ||
    [ "$package_version" != "$EXPECTED_PACKAGE_VERSION" ] ||
    [ "$patches_sha256" != "$ACTUAL_PATCHES_DIGEST" ] ||
    [ "$patch_application_sha256" != "$ACTUAL_PATCH_APPLICATION_DIGEST" ] ||
    [ "$zig_version" != "$ZIG_VERSION" ] ||
    [ "$zig_target" != "aarch64-macos" ] ||
    [ "$zig_sha256" != "$ZIG_AARCH64_MACOS_SHA256" ] ||
    [ "$zig_store_sha256" != "$ACTUAL_ZIG_STORE_DIGEST" ] ||
    [ "$metal_toolchain_build" != "$METAL_TOOLCHAIN_BUILD" ] ||
    [ "$metal_toolchain_dmg_sha256" != "$METAL_TOOLCHAIN_DMG_SHA256" ] ||
    [ "$ghostty_build_options" != "$EXPECTED_BUILD_OPTIONS" ] ||
    [ "$renderer_artifact_sha256" != "$ACTUAL_RENDERER_DIGEST" ] ||
    [ "$vt_artifact_sha256" != "$ACTUAL_VT_DIGEST" ] ||
    [ "$symbol_manifest_sha256" != "$ACTUAL_SYMBOL_DIGEST" ]; then
    echo "[!] Ghostty artifact provenance mismatch"
    exit 1
fi

for value in \
    "$renderer_target" \
    "$vt_target"; do
    if [ "$value" != "$GHOSTTY_BUILD_TARGET" ]; then
        echo "[!] unexpected Ghostty target: $value"
        exit 1
    fi
done
for value in "$renderer_cpu" "$vt_cpu"; do
    if [ "$value" != "$GHOSTTY_BUILD_CPU" ]; then
        echo "[!] unexpected Ghostty CPU policy: $value"
        exit 1
    fi
done
for value in "$renderer_optimize" "$vt_optimize"; do
    if [ "$value" != "$GHOSTTY_BUILD_OPTIMIZE" ]; then
        echo "[!] unexpected Ghostty optimization: $value"
        exit 1
    fi
done

metadata_value() {
    local file=$1
    local key=$2
    if [ "$(grep -c "^$key=" "$file" || true)" != 1 ]; then
        echo "[!] $file must contain exactly one $key" >&2
        return 1
    fi
    sed -n "s/^$key=//p" "$file"
}

for slice in "${IOS_SLICES[@]}"; do
    slice_metadata="$VENDOR_ROOT/libghostty-$slice.version"
    slice_dir="$RENDERER/$slice"
    if [ ! -f "$slice_metadata" ]; then
        echo "[!] metadata not found: $slice_metadata"
        exit 1
    fi
    slice_target=$(./scripts/native-build-options.sh target "$slice")
    expected_slice_options=$(metadata_value "$slice_metadata" ghostty_build_options)
    if [ "${KODOSI_GHOSTTY_REQUIRE_BUILD_PROVENANCE:-0}" = 1 ]; then
        expected_slice_options=$(./scripts/native-build-options.sh vector "$slice")
    fi
    expected_keys=
    for expectation in \
        "package_version=$EXPECTED_PACKAGE_VERSION" \
        "ghostty_commit=$EXPECTED_GHOSTTY_COMMIT" \
        "patches_sha256=$ACTUAL_PATCHES_DIGEST" \
        "patch_application_sha256=$ACTUAL_PATCH_APPLICATION_DIGEST" \
        "zig_version=$ZIG_VERSION" \
        "zig_target=aarch64-macos" \
        "zig_sha256=$ZIG_AARCH64_MACOS_SHA256" \
        "zig_store_sha256=$ACTUAL_ZIG_STORE_DIGEST" \
        "metal_toolchain_build=$METAL_TOOLCHAIN_BUILD" \
        "metal_toolchain_dmg_sha256=$METAL_TOOLCHAIN_DMG_SHA256" \
        "ghostty_build_options=$expected_slice_options" \
        "library_identifier=$slice" \
        "renderer_target=$slice_target" \
        "renderer_cpu=$GHOSTTY_BUILD_CPU" \
        "renderer_optimize=$GHOSTTY_BUILD_OPTIMIZE" \
        "vt_target=$slice_target" \
        "vt_cpu=$GHOSTTY_BUILD_CPU" \
        "vt_optimize=$GHOSTTY_BUILD_OPTIMIZE" \
        "combined_archive_sha256=$(shasum -a 256 "$slice_dir/libghostty.a" | awk '{print $1}')" \
        "slice_artifact_sha256=$(./scripts/artifact-digest.sh "$slice_dir")" \
        "symbol_manifest_sha256=$(shasum -a 256 "$VENDOR_ROOT/symbols/$slice-strong.txt" | awk '{print $1}')"; do
        key=${expectation%%=*}
        expected_keys+="$key"$'\n'
        if [ "$(metadata_value "$slice_metadata" "$key")" != "${expectation#*=}" ]; then
            echo "[!] $slice Ghostty artifact provenance mismatch: $key"
            exit 1
        fi
    done
    if [ "$(cut -d= -f1 "$slice_metadata" | LC_ALL=C sort)" != "$(printf '%s' "$expected_keys" | LC_ALL=C sort)" ]; then
        echo "[!] $slice metadata keys differ from the provenance contract"
        exit 1
    fi
done

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
for archive in \
    "$RENDERER_ARCHIVE" \
    "$VT_ARCHIVE" \
    "$RENDERER/ios-arm64/libghostty.a" \
    "$RENDERER/ios-arm64-simulator/libghostty.a"; do
    if [ "$(lipo -archs "$archive")" != arm64 ]; then
        echo "[!] Ghostty archive is not arm64-only: $archive"
        exit 1
    fi
done
require_build_version "$RENDERER_ARCHIVE" 1 13.0
require_build_version "$VT_ARCHIVE" 1 13.0
require_build_version "$RENDERER/ios-arm64/libghostty.a" 2 26.0
require_build_version "$RENDERER/ios-arm64-simulator/libghostty.a" 7 26.0
for slice in "${IOS_SLICES[@]}"; do
    for header in ghostty.h module.modulemap; do
        if ! cmp -s "$RENDERER/macos-arm64/Headers/$header" "$RENDERER/$slice/Headers/$header"; then
            echo "[!] $slice renderer header differs from macOS: $header"
            exit 1
        fi
    done
done

if ! cmp -s "$RENDERER_ARCHIVE" "$VT_ARCHIVE"; then
    echo "[!] renderer and VT artifact paths do not contain the same native image"
    exit 1
fi
if [ "$(sha256sum "$RENDERER_ARCHIVE" | awk '{print $1}')" != "$combined_archive_sha256" ]; then
    echo "[!] combined Ghostty archive digest does not match provenance"
    exit 1
fi

if [ ! -f "$VT/include/ghostty/vt.h" ] ||
    [ ! -f "$VT/include/module.modulemap" ] ||
    ! grep -Fq 'module GhosttyVt' "$VT/include/module.modulemap"; then
    echo "[!] VT headers or module map are incomplete"
    exit 1
fi

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-ghostty-verify.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT

for slice in macos-arm64 "${IOS_SLICES[@]}"; do
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
    -mmacosx-version-min=13.0 \
    -I"$RENDERER/macos-arm64/Headers" \
    -c "$TMP_DIR/renderer.c" \
    -o "$TMP_DIR/renderer.o"
xcrun clang \
    -arch arm64 \
    -mmacosx-version-min=13.0 \
    -I"$VT/include" \
    -c "$TMP_DIR/vt.c" \
    -o "$TMP_DIR/vt.o"
xcrun clang \
    -arch arm64 \
    -mmacosx-version-min=13.0 \
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
ios_link_probe ios-arm64 iphoneos arm64-apple-ios26.0
ios_link_probe ios-arm64-simulator iphonesimulator arm64-apple-ios26.0-simulator

for slice in macos-arm64 "${IOS_SLICES[@]}"; do
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
