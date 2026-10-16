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
source LinuxVtBuild.env
ARTIFACT_ROOT="${VENDOR_ROOT:-Vendor}/GhosttyVt/$GHOSTTY_LINUX_VT_LAYOUT"

if [ ! -d "$ARTIFACT_ROOT" ]; then
    echo "[!] Linux Ghostty VT artifact is missing: $ARTIFACT_ROOT" >&2
    exit 1
fi

ARCHIVE="$ARTIFACT_ROOT/lib/libghostty-vt.a"
SHARED_REAL="$ARTIFACT_ROOT/lib/libghostty-vt.so.0.1.0"
SHARED_SONAME="$ARTIFACT_ROOT/lib/libghostty-vt.so.0"
SHARED_LINK="$ARTIFACT_ROOT/lib/libghostty-vt.so"
HEADERS="$ARTIFACT_ROOT/include"
for path in \
    "$ARCHIVE" \
    "$SHARED_REAL" \
    "$HEADERS/ghostty/vt.h" \
    "$HEADERS/ghostty/vt/snapshot.h" \
    "$ARTIFACT_ROOT/symbols/strong.txt"; do
    if [ ! -f "$path" ]; then
        echo "[!] incomplete Linux Ghostty VT artifact: $path" >&2
        exit 1
    fi
done
if [ "$(readlink "$SHARED_SONAME")" != "libghostty-vt.so.0.1.0" ] ||
    [ "$(readlink "$SHARED_LINK")" != "libghostty-vt.so.0" ]; then
    echo "[!] Linux Ghostty VT shared-library link chain is invalid" >&2
    exit 1
fi
if ! readelf -d "$SHARED_REAL" | grep -Fq 'Library soname: [libghostty-vt.so.0]'; then
    echo "[!] Linux Ghostty VT shared library has an unexpected SONAME" >&2
    exit 1
fi
if ldd "$SHARED_REAL" 2>&1 | grep -Eq 'not found|invalid ELF header'; then
    echo "[!] Linux Ghostty VT shared library has unusable runtime dependencies" >&2
    exit 1
fi
if readelf -S "$SHARED_REAL" | grep -F '.debug_' >/dev/null; then
    echo "[!] Linux Ghostty VT shared library retains debug sections" >&2
    exit 1
fi
for artifact in "$ARCHIVE" "$SHARED_REAL"; do
    if strings "$artifact" | grep -F "$ROOT_DIR" >/dev/null; then
        echo "[!] Linux Ghostty VT artifact embeds the build repository path: $artifact" >&2
        exit 1
    fi
done

python3 - "$HEADERS/ghostty" "$SHARED_REAL" <<'PY'
import re
import subprocess
import sys
from pathlib import Path

headers = Path(sys.argv[1])
library = Path(sys.argv[2])
text = "\n".join(path.read_text() for path in headers.rglob("*.h"))
text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
text = re.sub(r"//.*", "", text)
declared = set(
    re.findall(
        r"\bGHOSTTY_API\b[^;{}]*?\b(ghostty_[A-Za-z0-9_]+)\s*\(",
        text,
        flags=re.S,
    )
)
declared -= {
    "ghostty_wasm_alloc",
    "ghostty_wasm_alloc_opaque",
    "ghostty_wasm_free",
    "ghostty_wasm_free_opaque",
    "ghostty_wasm_take_opaque",
}
exported = {
    line.split(maxsplit=2)[2]
    for line in subprocess.check_output(
        ["nm", "-D", "--defined-only", library],
        text=True,
    ).splitlines()
    if len(line.split(maxsplit=2)) == 3
}
if exported != declared:
    print(f"[!] unexpected shared exports: {sorted(exported - declared)}")
    print(f"[!] missing shared exports: {sorted(declared - exported)}")
    raise SystemExit(1)
PY

for symbol in \
    ghostty_terminal_new \
    ghostty_terminal_vt_write \
    ghostty_snapshot_encode_alloc \
    ghostty_snapshot_decoder_new_buf \
    ghostty_snapshot_decoder_decode; do
    if ! grep -Fxq "$symbol" "$ARTIFACT_ROOT/symbols/strong.txt"; then
        echo "[!] Linux Ghostty VT artifact is missing $symbol" >&2
        exit 1
    fi
done

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-ghostty-linux-verify.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT

cat > "$TMP_DIR/probe.c" <<'C'
#define GHOSTTY_STATIC
#include <ghostty/vt.h>

#include <stdlib.h>

int main(void) {
    GhosttyTerminal source = NULL;
    if (ghostty_terminal_new(NULL, &source, 80, 24) != GHOSTTY_SUCCESS) return 1;

    static const uint8_t payload[] = "hello\r\n";
    ghostty_terminal_vt_write(source, payload, sizeof(payload) - 1);

    uint8_t* snapshot = NULL;
    size_t snapshot_len = 0;
    if (ghostty_snapshot_encode_alloc(source, NULL, &snapshot, &snapshot_len) != GHOSTTY_SUCCESS)
        return 2;
    if (snapshot == NULL || snapshot_len == 0) return 3;

    GhosttySnapshotDecoder decoder = NULL;
    if (ghostty_snapshot_decoder_new_buf(NULL, &decoder, snapshot, snapshot_len) != GHOSTTY_SUCCESS)
        return 4;
    GhosttyTerminal restored = NULL;
    if (ghostty_snapshot_decoder_decode(decoder, &restored) != GHOSTTY_SUCCESS) return 5;
    size_t consumed = 0;
    if (ghostty_snapshot_decoder_get(
            decoder,
            GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET,
            &consumed) != GHOSTTY_SUCCESS ||
        consumed != snapshot_len) return 6;
    uint16_t cursor_y = 0;
    if (ghostty_terminal_get(restored, GHOSTTY_TERMINAL_DATA_CURSOR_Y, &cursor_y) != GHOSTTY_SUCCESS ||
        cursor_y != 1) return 7;

    ghostty_snapshot_decoder_free(decoder);
    ghostty_free(NULL, snapshot, snapshot_len);
    ghostty_terminal_free(restored);
    ghostty_terminal_free(source);
    return 0;
}
C

cc -std=c11 -Wall -Wextra -Werror -I"$HEADERS" \
    "$TMP_DIR/probe.c" "$ARCHIVE" -o "$TMP_DIR/probe-c"
c++ -std=c++23 -Wall -Wextra -Werror -x c++ -I"$HEADERS" \
    "$TMP_DIR/probe.c" -x none "$ARCHIVE" -o "$TMP_DIR/probe-cxx"
cc -std=c11 -Wall -Wextra -Werror -I"$HEADERS" \
    "$TMP_DIR/probe.c" \
    -L"$ARTIFACT_ROOT/lib" \
    -Wl,-rpath,"$ARTIFACT_ROOT/lib" \
    -lghostty-vt \
    -o "$TMP_DIR/probe-shared"
"$TMP_DIR/probe-c"
"$TMP_DIR/probe-cxx"
"$TMP_DIR/probe-shared"

echo "[*] verified Linux Ghostty VT artifact and snapshot round trip"
