#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

lock_value() {
    python3 -c '
import json
import sys

value = json.load(open("dependencies.lock.json", encoding="utf-8"))
try:
    for key in sys.argv[1:]:
        value = value[key]
except (KeyError, TypeError):
    value = None
if not isinstance(value, str) or not value:
    raise SystemExit("dependencies.lock.json has no value at " + "/".join(sys.argv[1:]))
sys.stdout.write(value)
' "$@"
}

CMAKE_VERSION=$(lock_value tools cmake version)
NINJA_VERSION=$(lock_value tools ninja version)
QT_VERSION=$(lock_value frameworks qt version)
# The aqtinstall release cannot read the Windows repository of the pinned Qt.
AQTINSTALL_COMMIT=$(lock_value tools aqtinstall commit)

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        HOST=linux
        EXE=
        TOOL_HOST=linux-x86_64
        CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz"
        NINJA_ARCHIVE="ninja-linux.zip"
        QT_HOST=linux
        QT_ARCH=linux_gcc_64
        QT_DIR=gcc_64
        ;;
    MINGW* | MSYS*)
        HOST=windows
        EXE=.exe
        # shellcheck disable=SC1091
        source ../../scripts/windows-env.sh
        case "$(kodosi_windows_arch)" in
            x64)
                TOOL_HOST=windows-x86_64
                CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-windows-x86_64.zip"
                NINJA_ARCHIVE="ninja-win.zip"
                QT_HOST=windows
                QT_ARCH=win64_msvc2022_64
                QT_DIR=msvc2022_64
                ;;
            arm64)
                TOOL_HOST=windows-arm64
                CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-windows-arm64.zip"
                NINJA_ARCHIVE="ninja-winarm64.zip"
                QT_HOST=windows_arm64
                QT_ARCH=win64_msvc2022_arm64
                QT_DIR=msvc2022_arm64
                ;;
            *) exit 1 ;;
        esac
        ;;
    *)
        echo "unsupported install host: $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

CMAKE_SHA256=$(lock_value tools cmake sha256 "$TOOL_HOST")
NINJA_SHA256=$(lock_value tools ninja sha256 "$TOOL_HOST")
CMAKE_URL="https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/${CMAKE_ARCHIVE}"
CMAKE_ROOT="$PWD/.tools/cmake"
NINJA_URL="https://github.com/ninja-build/ninja/releases/download/v${NINJA_VERSION}/${NINJA_ARCHIVE}"
NINJA_ROOT="$PWD/.tools/ninja"
QT_ROOT="$PWD/.tools/Qt/$QT_DIR"
AQT_ROOT="$PWD/.tools/aqt-${AQTINSTALL_COMMIT:0:12}"
DOWNLOAD_ROOT="$PWD/.tools/downloads"

mkdir -p "$DOWNLOAD_ROOT"

download() {
    local name=$1 url=$2 sha256=$3 archive temporary
    archive="$DOWNLOAD_ROOT/$name"
    if [ ! -f "$archive" ] ||
        ! printf '%s  %s\n' "$sha256" "$archive" | sha256sum -c - >/dev/null 2>&1; then
        temporary="$archive.tmp.$$"
        curl -fL \
            --retry 5 \
            --retry-all-errors \
            --retry-delay 2 \
            --connect-timeout 20 \
            -o "$temporary" \
            "$url" >&2
        if ! printf '%s  %s\n' "$sha256" "$temporary" | sha256sum -c - >&2; then
            rm -f "$temporary"
            return 1
        fi
        mv "$temporary" "$archive"
    fi
    printf '%s\n' "$archive"
}

unpack() {
    local archive=$1 destination=$2 strip=$3
    case "$archive" in
        *.tar.gz) tar -xzf "$archive" -C "$destination" --strip-components="$strip" ;;
        *.zip)
            if [ "$HOST" = windows ]; then
                /c/Windows/System32/tar.exe -xf "$(cygpath -w "$archive")" -C "$(cygpath -w "$destination")" \
                    --strip-components="$strip"
            else
                python3 -m zipfile -e "$archive" "$destination"
            fi
            ;;
    esac
}

cmake_ready() {
    [ -x "$1/bin/cmake$EXE" ] &&
        [ -x "$1/bin/ctest$EXE" ] &&
        [ "$("$1/bin/cmake$EXE" --version | awk 'NR == 1 { print $3 }' | tr -d '\r')" = "$CMAKE_VERSION" ]
}

if ! cmake_ready "$CMAKE_ROOT"; then
    archive=$(download "$CMAKE_ARCHIVE" "$CMAKE_URL" "$CMAKE_SHA256")
    stage="$PWD/.tools/cmake.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    unpack "$archive" "$stage" 1
    if ! cmake_ready "$stage"; then
        echo "CMake install verification failed" >&2
        exit 1
    fi
    rm -rf "$CMAKE_ROOT"
    mv "$stage" "$CMAKE_ROOT"
fi

ninja_ready() {
    [ -x "$1/ninja$EXE" ] &&
        [ "$("$1/ninja$EXE" --version | tr -d '\r')" = "$NINJA_VERSION" ]
}

if ! ninja_ready "$NINJA_ROOT"; then
    archive=$(download "ninja-${NINJA_VERSION}-${NINJA_ARCHIVE#ninja-}" "$NINJA_URL" "$NINJA_SHA256")
    stage="$PWD/.tools/ninja.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    unpack "$archive" "$stage" 0
    chmod +x "$stage/ninja$EXE"
    if ! ninja_ready "$stage"; then
        echo "Ninja install verification failed" >&2
        exit 1
    fi
    rm -rf "$NINJA_ROOT"
    mv "$stage" "$NINJA_ROOT"
fi

qt_ready() {
    local root=$1
    if [ "$HOST" = windows ]; then
        [ -f "$root/lib/cmake/Qt6/Qt6Config.cmake" ] &&
            [ -x "$root/bin/qmllint.exe" ] &&
            [ -f "$root/bin/Qt6Core.dll" ] &&
            [ -f "$root/bin/Qt6Quick.dll" ] &&
            [ "$("$root/bin/qtpaths6.exe" --qt-version | tr -d '\r')" = "$QT_VERSION" ]
    else
        [ -f "$root/lib/cmake/Qt6/Qt6Config.cmake" ] &&
            [ -x "$root/bin/qmllint" ] &&
            [ -f "$root/lib/libQt6Core.so.$QT_VERSION" ] &&
            [ -f "$root/lib/libQt6Quick.so.$QT_VERSION" ] &&
            [ "$("$root/bin/qtpaths6" --qt-version)" = "$QT_VERSION" ]
    fi
}

if ! qt_ready "$QT_ROOT"; then
    command -v uv >/dev/null 2>&1 || {
        echo "uv is required to create the isolated aqtinstall environment" >&2
        exit 1
    }
    environment="$AQT_ROOT/bin"
    [ "$HOST" = windows ] && environment="$AQT_ROOT/Scripts"
    aqt="$environment/aqt$EXE"
    if [ ! -x "$aqt" ]; then
        uv venv --clear --quiet --python 3.12 "$AQT_ROOT"
        uv pip install --quiet --python "$environment/python$EXE" \
            "aqtinstall @ git+https://github.com/miurahr/aqtinstall@${AQTINSTALL_COMMIT}"
    fi

    stage="$PWD/.tools/Qt.stage.$$"
    rm -rf "$stage"
    "$aqt" install-qt "$QT_HOST" desktop "$QT_VERSION" "$QT_ARCH" --outputdir "$stage"
    staged_qt="$stage/$QT_VERSION/$QT_DIR"
    if ! qt_ready "$staged_qt"; then
        echo "Qt install verification failed" >&2
        exit 1
    fi
    mkdir -p "$(dirname "$QT_ROOT")"
    rm -rf "$QT_ROOT"
    mv "$staged_qt" "$QT_ROOT"
    rm -rf "$stage"
fi

echo "CMake $CMAKE_VERSION, Ninja $NINJA_VERSION, and Qt $QT_VERSION ($QT_DIR) are ready"
