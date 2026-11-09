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

ROOT_DIR=$(pwd)
# shellcheck disable=SC1091
source AndroidVtBuild.env
NDK=$("$ROOT_DIR/scripts/find-android-ndk.sh")
NDK_BIN=$("$ROOT_DIR/scripts/find-android-ndk.sh" bin)
ARTIFACT_ROOT="${VENDOR_ROOT:-Vendor}/GhosttyVt/$GHOSTTY_ANDROID_VT_LAYOUT"
ARCHIVE="$ARTIFACT_ROOT/lib/libghostty-vt.a"
HEADERS="$ARTIFACT_ROOT/include"

for path in \
    "$ARCHIVE" \
    "$HEADERS/ghostty/vt.h" \
    "$HEADERS/ghostty/vt/snapshot.h" \
    "$ARTIFACT_ROOT/symbols/strong.txt"; do
    if [ ! -f "$path" ]; then
        echo "[!] incomplete Android Ghostty VT artifact: $path" >&2
        exit 1
    fi
done
if [ "$(find "$ARTIFACT_ROOT/lib" -type f | wc -l | tr -d ' ')" != 1 ]; then
    echo "[!] the Android Ghostty VT artifact holds only its static archive" >&2
    exit 1
fi
if "$NDK_BIN/llvm-readelf" -S "$ARCHIVE" | grep -F '.debug_' >/dev/null; then
    echo "[!] Android Ghostty VT archive retains debug sections" >&2
    exit 1
fi
if ! "$NDK_BIN/llvm-readelf" -h "$ARCHIVE" | grep -E 'Machine: +AArch64' >/dev/null; then
    echo "[!] Android Ghostty VT archive is not AArch64" >&2
    exit 1
fi
for build_path in "$ROOT_DIR" "$NDK"; do
    if LC_ALL=C grep -aF "$build_path" "$ARCHIVE" >/dev/null; then
        echo "[!] Android Ghostty VT archive embeds a build path: $build_path" >&2
        exit 1
    fi
done
for symbol in \
    ghostty_terminal_new \
    ghostty_terminal_vt_write \
    ghostty_render_state_update \
    ghostty_key_encoder_encode \
    ghostty_snapshot_encode_alloc \
    ghostty_snapshot_decoder_new_buf \
    ghostty_snapshot_decoder_decode; do
    if ! grep -Fxq "$symbol" "$ARTIFACT_ROOT/symbols/strong.txt"; then
        echo "[!] Android Ghostty VT artifact is missing $symbol" >&2
        exit 1
    fi
done

API_LEVEL=${GHOSTTY_ANDROID_VT_TARGET##*.}
TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-ghostty-android-verify.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT

cat > "$TMP_DIR/probe.c" <<'C'
#define GHOSTTY_STATIC
#include <ghostty/vt.h>

int main(void) {
    GhosttyTerminal source = NULL;
    if (ghostty_terminal_new(NULL, &source, 80, 24) != GHOSTTY_SUCCESS) return 1;
    static const uint8_t payload[] = "hello\r\n";
    ghostty_terminal_vt_write(source, payload, sizeof(payload) - 1);
    uint8_t* snapshot = NULL;
    size_t snapshot_len = 0;
    if (ghostty_snapshot_encode_alloc(source, NULL, &snapshot, &snapshot_len) != GHOSTTY_SUCCESS)
        return 2;
    GhosttySnapshotDecoder decoder = NULL;
    if (ghostty_snapshot_decoder_new_buf(NULL, &decoder, snapshot, snapshot_len) != GHOSTTY_SUCCESS)
        return 3;
    GhosttyTerminal restored = NULL;
    if (ghostty_snapshot_decoder_decode(decoder, &restored) != GHOSTTY_SUCCESS) return 4;
    GhosttyRenderState state = NULL;
    if (ghostty_render_state_new(NULL, &state) != GHOSTTY_SUCCESS) return 5;
    if (ghostty_render_state_update(state, restored) != GHOSTTY_SUCCESS) return 6;
    ghostty_render_state_free(state);
    ghostty_snapshot_decoder_free(decoder);
    ghostty_free(NULL, snapshot, snapshot_len);
    ghostty_terminal_free(restored);
    ghostty_terminal_free(source);
    return 0;
}
C

"$NDK_BIN/aarch64-linux-android$API_LEVEL-clang" -std=c11 -Wall -Wextra -Werror -I"$HEADERS" \
    "$TMP_DIR/probe.c" "$ARCHIVE" -o "$TMP_DIR/probe-c"
"$NDK_BIN/aarch64-linux-android$API_LEVEL-clang++" -std=c++20 -Wall -Wextra -Werror -x c++ -I"$HEADERS" \
    "$TMP_DIR/probe.c" -x none "$ARCHIVE" -o "$TMP_DIR/probe-cxx"

echo "[*] verified Android Ghostty VT artifact: it links for Android API $API_LEVEL"
