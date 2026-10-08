#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

CMAKE_VERSION=4.4.3
CMAKE_SHA256=d6c83076c575bc00b823522ac974bda66d0af05d6ddc30e739c12385cf32c6cc
CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz"
CMAKE_URL="https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/${CMAKE_ARCHIVE}"
CMAKE_ROOT="$PWD/.tools/cmake-${CMAKE_VERSION}"
DOWNLOAD_ROOT="$PWD/.tools/downloads"
NINJA_VERSION=1.13.2
NINJA_SHA256=5749cbc4e668273514150a80e387a957f933c6ed3f5f11e03fb30955e2bbead6
NINJA_ARCHIVE="ninja-linux.zip"
NINJA_URL="https://github.com/ninja-build/ninja/releases/download/v${NINJA_VERSION}/${NINJA_ARCHIVE}"
NINJA_ROOT="$PWD/.tools/ninja-${NINJA_VERSION}"

if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
    echo "unsupported bootstrap host: $(uname -s)-$(uname -m)" >&2
    exit 1
fi

mkdir -p "$DOWNLOAD_ROOT"

cmake_ready() {
    [ -x "$CMAKE_ROOT/bin/cmake" ] &&
        [ -x "$CMAKE_ROOT/bin/ctest" ] &&
        [ "$("$CMAKE_ROOT/bin/cmake" --version | awk 'NR == 1 { print $3 }')" = "$CMAKE_VERSION" ]
}

if ! cmake_ready; then
    archive="$DOWNLOAD_ROOT/$CMAKE_ARCHIVE"
    if [ ! -f "$archive" ] ||
        ! printf '%s  %s\n' "$CMAKE_SHA256" "$archive" | sha256sum -c - >/dev/null 2>&1; then
        temporary="$archive.tmp.$$"
        trap 'rm -f "$temporary"' EXIT
        curl -fL \
            --retry 5 \
            --retry-all-errors \
            --retry-delay 2 \
            --connect-timeout 20 \
            -o "$temporary" \
            "$CMAKE_URL"
        printf '%s  %s\n' "$CMAKE_SHA256" "$temporary" | sha256sum -c -
        mv "$temporary" "$archive"
        trap - EXIT
    fi

    stage="$PWD/.tools/cmake-${CMAKE_VERSION}.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    tar -xzf "$archive" -C "$stage" --strip-components=1
    if [ ! -x "$stage/bin/cmake" ] ||
        [ ! -x "$stage/bin/ctest" ] ||
        [ "$("$stage/bin/cmake" --version | awk 'NR == 1 { print $3 }')" != "$CMAKE_VERSION" ]; then
        echo "CMake bootstrap verification failed" >&2
        exit 1
    fi
    rm -rf "$CMAKE_ROOT"
    mv "$stage" "$CMAKE_ROOT"
fi

ninja_ready() {
    [ -x "$NINJA_ROOT/ninja" ] &&
        [ "$("$NINJA_ROOT/ninja" --version)" = "$NINJA_VERSION" ]
}

if ! ninja_ready; then
    archive="$DOWNLOAD_ROOT/ninja-${NINJA_VERSION}-linux.zip"
    if [ ! -f "$archive" ] ||
        ! printf '%s  %s\n' "$NINJA_SHA256" "$archive" | sha256sum -c - >/dev/null 2>&1; then
        temporary="$archive.tmp.$$"
        trap 'rm -f "$temporary"' EXIT
        curl -fL \
            --retry 5 \
            --retry-all-errors \
            --retry-delay 2 \
            --connect-timeout 20 \
            -o "$temporary" \
            "$NINJA_URL"
        printf '%s  %s\n' "$NINJA_SHA256" "$temporary" | sha256sum -c -
        mv "$temporary" "$archive"
        trap - EXIT
    fi

    stage="$PWD/.tools/ninja-${NINJA_VERSION}.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    python3 -m zipfile -e "$archive" "$stage"
    chmod +x "$stage/ninja"
    if [ "$("$stage/ninja" --version)" != "$NINJA_VERSION" ]; then
        echo "Ninja bootstrap verification failed" >&2
        exit 1
    fi
    rm -rf "$NINJA_ROOT"
    mv "$stage" "$NINJA_ROOT"
fi

QT_ROOT="$PWD/.tools/Qt/6.11.2/gcc_64"
qt_ready() {
    [ -f "$QT_ROOT/lib/cmake/Qt6/Qt6Config.cmake" ] &&
        [ -x "$QT_ROOT/bin/qmllint" ] &&
        [ -f "$QT_ROOT/lib/libQt6Core.so.6.11.2" ] &&
        [ -f "$QT_ROOT/lib/libQt6Quick.so.6.11.2" ] &&
        [ "$("$QT_ROOT/bin/qtpaths6" --qt-version)" = "6.11.2" ]
}

if ! qt_ready; then
    command -v uv >/dev/null 2>&1 || {
        echo "uv is required to create the isolated aqtinstall environment" >&2
        exit 1
    }
    if [ ! -x "$PWD/.tools/aqt-3.3.0/bin/aqt" ]; then
        uv venv --clear --quiet --python python3 "$PWD/.tools/aqt-3.3.0"
    fi
    uv pip install --quiet --python "$PWD/.tools/aqt-3.3.0/bin/python" \
        "aqtinstall==3.3.0"

    stage="$PWD/.tools/Qt.stage.$$"
    rm -rf "$stage"
    "$PWD/.tools/aqt-3.3.0/bin/aqt" install-qt linux desktop 6.11.2 linux_gcc_64 \
        --outputdir "$stage"
    staged_qt="$stage/6.11.2/gcc_64"
    if [ ! -f "$staged_qt/lib/cmake/Qt6/Qt6Config.cmake" ] ||
        [ ! -x "$staged_qt/bin/qmllint" ] ||
        [ ! -f "$staged_qt/lib/libQt6Core.so.6.11.2" ] ||
        [ ! -f "$staged_qt/lib/libQt6Quick.so.6.11.2" ] ||
        [ "$("$staged_qt/bin/qtpaths6" --qt-version)" != "6.11.2" ]; then
        echo "Qt bootstrap verification failed" >&2
        exit 1
    fi
    mkdir -p "$(dirname "$QT_ROOT")"
    rm -rf "$QT_ROOT"
    mv "$staged_qt" "$QT_ROOT"
    rm -rf "$stage"
fi

echo "CMake $CMAKE_VERSION, Ninja $NINJA_VERSION, and Qt 6.11.2 are ready"
