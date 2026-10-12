#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
ARTIFACT_ROOT=${KODOSI_GHOSTTY_LINUX_ARTIFACT_ROOT:-Vendor/GhosttyVt/linux-x86_64}

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
    "$HEADERS/ghostty/vt/checkpoint.h" \
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
    ghostty_checkpoint_schema \
    ghostty_checkpoint_encode_alloc \
    ghostty_checkpoint_restore; do
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
    if (ghostty_checkpoint_schema() != GHOSTTY_CHECKPOINT_SCHEMA_VERSION) return 1;

    GhosttyTerminal source = NULL;
    GhosttyTerminal restored = NULL;
    if (ghostty_terminal_new(NULL, &source, 80, 24) != GHOSTTY_SUCCESS) return 2;
    if (ghostty_terminal_new(NULL, &restored, 80, 24) != GHOSTTY_SUCCESS) return 3;

    static const uint8_t payload[] = "hello\r\n";
    ghostty_terminal_vt_write(source, payload, sizeof(payload) - 1);

    GhosttyCheckpointEncodeOptions encode = GHOSTTY_CHECKPOINT_ENCODE_OPTIONS_INIT;
    GhosttyCheckpointInfo encoded_info = GHOSTTY_INIT_SIZED(GhosttyCheckpointInfo);
    uint8_t* checkpoint = NULL;
    size_t checkpoint_len = 0;
    if (ghostty_checkpoint_encode_alloc(
            source,
            &encode,
            NULL,
            &checkpoint,
            &checkpoint_len,
            &encoded_info) != GHOSTTY_SUCCESS) return 4;
    if (checkpoint == NULL || checkpoint_len == 0) return 5;

    GhosttyCheckpointRestoreOptions restore = GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT;
    GhosttyCheckpointInfo restored_info = GHOSTTY_INIT_SIZED(GhosttyCheckpointInfo);
    if (ghostty_checkpoint_restore(
            restored,
            checkpoint,
            checkpoint_len,
            &restore,
            &restored_info) != GHOSTTY_SUCCESS) return 6;
    if (restored_info.schema_version != GHOSTTY_CHECKPOINT_SCHEMA_VERSION) return 7;

    ghostty_free(NULL, checkpoint, checkpoint_len);
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

echo "[*] verified Linux Ghostty VT artifact and schema-v2 checkpoint round trip"
