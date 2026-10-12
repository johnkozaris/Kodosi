#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
if [ -z "$SOURCE_DIR" ] || [ ! -d "$SOURCE_DIR/zig-pkg" ]; then
    echo "Usage: $0 <prepared_source_dir>" >&2
    exit 1
fi

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source LinuxVtBuild.env
# shellcheck disable=SC1091
source PythonTools.env

if [ "$ZIG_TARGET" != "x86_64-linux" ]; then
    echo "[!] Linux source verification requires the Linux Zig toolchain" >&2
    exit 1
fi

PYTHON_ROOT="$ROOT_DIR/.tools/python-jsonschema-$JSONSCHEMA_VERSION"
python_ready() {
    [ -x "$PYTHON_ROOT/bin/python" ] &&
        [ "$("$PYTHON_ROOT/bin/python" -c \
            'import importlib.metadata; print(importlib.metadata.version("jsonschema"))')" = \
            "$JSONSCHEMA_VERSION" ]
}
if ! python_ready; then
    command -v uv >/dev/null 2>&1 || {
        echo "[!] uv is required for the schema verifier environment" >&2
        exit 1
    }
    STAGE="$PYTHON_ROOT.stage.$$"
    rm -rf "$STAGE"
    uv venv --quiet --python python3 "$STAGE"
    uv pip install --quiet --python "$STAGE/bin/python" \
        "jsonschema==$JSONSCHEMA_VERSION"
    if [ "$("$STAGE/bin/python" -c \
        'import importlib.metadata; print(importlib.metadata.version("jsonschema"))')" != \
        "$JSONSCHEMA_VERSION" ]; then
        echo "[!] jsonschema verifier bootstrap failed" >&2
        exit 1
    fi
    rm -rf "$PYTHON_ROOT"
    mv "$STAGE" "$PYTHON_ROOT"
fi

ZIG_BIN="$ROOT_DIR/.tools/zig-$ZIG_VERSION-$ZIG_SHA256/zig"
if [ ! -x "$ZIG_BIN" ] || [ "$("$ZIG_BIN" version)" != "$ZIG_VERSION" ]; then
    echo "[!] pinned Zig toolchain is missing" >&2
    exit 1
fi

(
    cd "$SOURCE_DIR"
    "$ZIG_BIN" build test-lib-vt \
        "-Dtarget=$GHOSTTY_LINUX_VT_TARGET" \
        "-Dcpu=$GHOSTTY_LINUX_VT_CPU" \
        --system "$SOURCE_DIR/zig-pkg"
    PATH="$PYTHON_ROOT/bin:$PATH" "$ZIG_BIN" build test-lib-vt-schema \
        "-Dtarget=$GHOSTTY_LINUX_VT_TARGET" \
        "-Dcpu=$GHOSTTY_LINUX_VT_CPU" \
        "-Doptimize=$GHOSTTY_LINUX_VT_OPTIMIZE" \
        --system "$SOURCE_DIR/zig-pkg"
)

echo "[*] verified patched Linux Ghostty VT source tests and ABI schema"
