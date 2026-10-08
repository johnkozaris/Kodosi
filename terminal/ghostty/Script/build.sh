#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

usage() {
    cat <<'EOF'
Usage: ./Script/build.sh [options]

Options:
  --source <path>       Use an existing clean Ghostty checkout as the source cache.
  --platform <name>     Build macos or linux (defaults to the current host).
  --skip-tests          Skip package verification after native artifacts build.
  -h, --help            Show this help.

The macOS build produces one arm64 native image containing renderer and VT APIs.
The Linux build produces an x86_64 VT library for the Rust authority and Qt
renderer. Bootstrap may fetch pinned source/tool dependencies; native builds
then run against the prepared local Zig package store with network disabled.
EOF
}

ROOT_DIR=$(pwd)
mkdir -p "$ROOT_DIR/build"
LOCK_DIR="$ROOT_DIR/build/.native-build.lock"
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    LOCK_PID=$(cat "$LOCK_DIR/pid" 2>/dev/null || true)
    if [ -n "$LOCK_PID" ] && kill -0 "$LOCK_PID" 2>/dev/null; then
        echo "[!] Ghostty native build $LOCK_PID is already running"
        exit 1
    fi
    rm -rf "$LOCK_DIR"
    mkdir "$LOCK_DIR"
fi
printf '%s\n' "$$" > "$LOCK_DIR/pid"

cleanup() {
    if [ "${PLATFORM_GROUP:-}" = macos ]; then
        "$ROOT_DIR/Script/bootstrap-metal.sh" unmount || true
    fi
    rm -rf "$LOCK_DIR"
}
trap cleanup EXIT
SOURCE_CACHE="$ROOT_DIR/References/ghostty-upstream"
SOURCE_SUPPLIED=0
SKIP_TESTS=0
case "$(uname -s)" in
    Darwin) PLATFORM_GROUP=macos ;;
    Linux) PLATFORM_GROUP=linux ;;
    *)
        echo "[!] unsupported build host: $(uname -s)" >&2
        exit 1
        ;;
esac

while [ $# -gt 0 ]; do
    case "$1" in
        --source)
            [ $# -ge 2 ] || { printf '%s\n' '--source requires a path' >&2; exit 1; }
            SOURCE_CACHE="$2"
            SOURCE_SUPPLIED=1
            shift 2
            ;;
        --platform)
            PLATFORM_GROUP="$2"
            shift 2
            ;;
        --skip-tests)
            SKIP_TESTS=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "[!] unknown argument: $1"
            usage
            exit 1
            ;;
    esac
done

case "$PLATFORM_GROUP" in
    macos)
        GHOSTTY_REF_FILE="$ROOT_DIR/MacOSGhostty.ref"
        if [ "$(uname -s)-$(uname -m)" != "Darwin-arm64" ]; then
            echo "[!] macOS artifacts require an Apple silicon build host" >&2
            exit 1
        fi
        ;;
    linux)
        GHOSTTY_REF_FILE="$ROOT_DIR/LinuxGhostty.ref"
        if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
            echo "[!] Linux artifacts require an x86_64 Linux build host" >&2
            exit 1
        fi
        ;;
    *)
        echo "[!] unsupported platform: $PLATFORM_GROUP" >&2
        exit 1
        ;;
esac
if [ "$(wc -c < "$GHOSTTY_REF_FILE" | tr -d '[:space:]')" != 41 ] ||
    ! grep -Eq '^[0-9a-f]{40}$' "$GHOSTTY_REF_FILE"; then
    echo "[!] platform Ghostty ref must contain one lowercase commit and newline: $GHOSTTY_REF_FILE" >&2
    exit 1
fi
GHOSTTY_REF=$(tr -d '[:space:]' < "$GHOSTTY_REF_FILE")

if [ -e "$SOURCE_CACHE" ] || [ -L "$SOURCE_CACHE" ]; then
    if [ ! -e "$SOURCE_CACHE/.git" ] || ! git -C "$SOURCE_CACHE" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        printf '[!] source cache is not a Git checkout; left untouched: %s\n' "$SOURCE_CACHE" >&2
        exit 1
    fi
elif [ "$SOURCE_SUPPLIED" -eq 1 ]; then
    printf '[!] supplied source checkout does not exist: %s\n' "$SOURCE_CACHE" >&2
    exit 1
else
    mkdir -p "$(dirname "$SOURCE_CACHE")"
    git clone https://github.com/ghostty-org/ghostty "$SOURCE_CACHE"
fi

SOURCE_CACHE=$(cd "$SOURCE_CACHE" && pwd -P)
SOURCE_DIR="$ROOT_DIR/build/ghostty-source"
if [ "$SOURCE_CACHE" = "$SOURCE_DIR" ] || [[ "$SOURCE_CACHE" == "$SOURCE_DIR/"* ]] ||
    { [ -e "$SOURCE_DIR" ] && [ "$SOURCE_CACHE" -ef "$SOURCE_DIR" ]; }; then
    printf '[!] source cache overlaps the disposable build checkout; left untouched: %s\n' "$SOURCE_CACHE" >&2
    exit 1
fi

ZIG_BIN=$("$ROOT_DIR/Script/bootstrap-zig.sh")
PATH="$(dirname "$ZIG_BIN"):$PATH"
export PATH
if [ "$PLATFORM_GROUP" = macos ]; then
    METAL_TOOLCHAIN=$("$ROOT_DIR/Script/bootstrap-metal.sh" mount)
    export GHOSTTY_METAL="$METAL_TOOLCHAIN/usr/bin/metal"
    export GHOSTTY_METALLIB="$METAL_TOOLCHAIN/usr/bin/metallib"
fi

if ! git -C "$SOURCE_CACHE" cat-file -e "$GHOSTTY_REF^{commit}" 2>/dev/null; then
    git -C "$SOURCE_CACHE" fetch origin "$GHOSTTY_REF"
fi
if [ "$(git -C "$SOURCE_CACHE" rev-parse "$GHOSTTY_REF^{commit}")" != "$GHOSTTY_REF" ]; then
    echo "[!] Ghostty ref did not resolve exactly"
    exit 1
fi

rm -rf "$SOURCE_DIR"
git clone --quiet --no-checkout "$SOURCE_CACHE" "$SOURCE_DIR"
git -C "$SOURCE_DIR" checkout --quiet --detach "$GHOSTTY_REF"
if [ -n "$(git -C "$SOURCE_DIR" status --porcelain)" ]; then
    echo "[!] isolated Ghostty source is dirty before patching"
    exit 1
fi

"$ROOT_DIR/Script/apply-patches.sh" "$SOURCE_DIR"
PATCH_APPLICATION_DIGEST=$(
    "$ROOT_DIR/Script/patch-application-digest.sh" "$SOURCE_DIR" "$GHOSTTY_REF"
)
ZIG_STORE_DIGEST=$("$ROOT_DIR/Script/prepare-zig-store.sh" "$SOURCE_DIR")

if [ "$PLATFORM_GROUP" = linux ]; then
    "$ROOT_DIR/Script/build-linux-vt-artifact.sh" \
        "$SOURCE_DIR" \
        "$PATCH_APPLICATION_DIGEST" \
        "$ZIG_STORE_DIGEST" \
        "$SKIP_TESTS"
    exit 0
fi

STAGED_DIR="$ROOT_DIR/build/artifacts"
"$ROOT_DIR/Script/build-ghostty.sh" "$SOURCE_DIR" aarch64-macos "$STAGED_DIR/macos-arm64"

VENDOR_STAGE=$(mktemp -d "$ROOT_DIR/build/vendor-stage.XXXXXX")
cleanup_vendor_stage() {
    rm -rf "$VENDOR_STAGE"
}
trap 'cleanup_vendor_stage; cleanup' EXIT

"$ROOT_DIR/Script/package-artifacts.sh" \
    "$STAGED_DIR" \
    "$VENDOR_STAGE/GhosttyKit.xcframework" \
    "$VENDOR_STAGE/GhosttyVt/macos-arm64" \
    "$VENDOR_STAGE/symbols"

LINUX_ARTIFACT="$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64"
LINUX_METADATA="$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64.version"
if [ -e "$LINUX_ARTIFACT" ] || [ -e "$LINUX_METADATA" ]; then
    if [ ! -d "$LINUX_ARTIFACT" ] || [ ! -f "$LINUX_METADATA" ]; then
        echo "[!] Linux VT artifact and metadata must both exist" >&2
        exit 1
    fi
    cp -R "$LINUX_ARTIFACT" "$VENDOR_STAGE/GhosttyVt/linux-x86_64"
    cp "$LINUX_METADATA" "$VENDOR_STAGE/GhosttyVt/linux-x86_64.version"
    KODOSI_GHOSTTY_SKIP_NATIVE_PROBES=1 \
        KODOSI_GHOSTTY_LINUX_ARTIFACT_ROOT="$VENDOR_STAGE/GhosttyVt/linux-x86_64" \
        KODOSI_GHOSTTY_LINUX_METADATA="$VENDOR_STAGE/GhosttyVt/linux-x86_64.version" \
        "$ROOT_DIR/Script/verify-linux-vt-artifact.sh"
fi

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source NativeBuild.env
RENDERER_DIGEST=$("$ROOT_DIR/Script/artifact-digest.sh" "$VENDOR_STAGE/GhosttyKit.xcframework")
VT_DIGEST=$("$ROOT_DIR/Script/artifact-digest.sh" "$VENDOR_STAGE/GhosttyVt/macos-arm64")
COMBINED_ARCHIVE=$(find "$VENDOR_STAGE/GhosttyKit.xcframework" -name libghostty.a -type f -print -quit)
COMBINED_ARCHIVE_DIGEST=$(shasum -a 256 "$COMBINED_ARCHIVE" | awk '{print $1}')
PATCHES_DIGEST=$("$ROOT_DIR/Script/artifact-digest.sh" Patches/ghostty)
SYMBOLS_DIGEST=$("$ROOT_DIR/Script/artifact-digest.sh" "$VENDOR_STAGE/symbols")
GHOSTTY_BUILD_OPTION_VECTOR=$("$ROOT_DIR/Script/native-build-options.sh" vector)
cat > "$VENDOR_STAGE/libghostty.version" <<EOF
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$GHOSTTY_REF
patches_sha256=$PATCHES_DIGEST
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_store_sha256=$ZIG_STORE_DIGEST
metal_toolchain_build=$METAL_TOOLCHAIN_BUILD
metal_toolchain_dmg_sha256=$METAL_TOOLCHAIN_DMG_SHA256
combined_archive_sha256=$COMBINED_ARCHIVE_DIGEST
ghostty_build_options=$GHOSTTY_BUILD_OPTION_VECTOR
renderer_target=$GHOSTTY_BUILD_TARGET
renderer_cpu=$GHOSTTY_BUILD_CPU
renderer_optimize=$GHOSTTY_BUILD_OPTIMIZE
renderer_artifact_sha256=$RENDERER_DIGEST
vt_target=$GHOSTTY_BUILD_TARGET
vt_cpu=$GHOSTTY_BUILD_CPU
vt_optimize=$GHOSTTY_BUILD_OPTIMIZE
vt_artifact_sha256=$VT_DIGEST
symbol_manifest_sha256=$SYMBOLS_DIGEST
EOF

if [ "$SKIP_TESTS" -eq 0 ]; then
    KODOSI_GHOSTTY_REQUIRE_BUILD_PROVENANCE=1 \
        KODOSI_GHOSTTY_VENDOR_ROOT="$VENDOR_STAGE" \
        "$ROOT_DIR/Script/verify-vendored-artifact.sh"
    KODOSI_GHOSTTY_VENDOR_ROOT="$VENDOR_STAGE" \
        "$ROOT_DIR/Script/sanitize-native-boundary.sh"
fi

if [ ! -e "$ROOT_DIR/Vendor" ]; then
    mv "$VENDOR_STAGE" "$ROOT_DIR/Vendor"
else
    VENDOR_SWAP_HELPER="$ROOT_DIR/build/vendor-swap"
    xcrun clang -x c -o "$VENDOR_SWAP_HELPER" - <<'C'
#include <stdio.h>
#include <sys/stdio.h>

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    if (renamex_np(argv[1], argv[2], RENAME_SWAP) == 0) return 0;
    perror("renamex_np");
    return 1;
}
C
    "$VENDOR_SWAP_HELPER" "$VENDOR_STAGE" "$ROOT_DIR/Vendor"
    rm -rf "$VENDOR_STAGE"
fi

if [ "$SKIP_TESTS" -eq 0 ]; then
    "$ROOT_DIR/Script/test.sh"
    swift test
fi

if [ -d "$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64" ]; then
    KODOSI_GHOSTTY_SKIP_NATIVE_PROBES=1 \
        "$ROOT_DIR/Script/verify-linux-vt-artifact.sh"
fi

echo "[*] renderer: Vendor/GhosttyKit.xcframework"
echo "[*] VT: Vendor/GhosttyVt/macos-arm64"
