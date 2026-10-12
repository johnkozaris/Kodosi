#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

CMAKE_VERSION=4.4.3
NINJA_VERSION=1.13.2
QT_VERSION=6.12.0
# aqtinstall's release cannot read Qt 6.12's Windows repository; this commit can.
AQTINSTALL_COMMIT=076e1659807d0b362a3ed684d54c2e9c775eb9c7

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        HOST=linux
        EXE=
        CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz"
        CMAKE_SHA256=d6c83076c575bc00b823522ac974bda66d0af05d6ddc30e739c12385cf32c6cc
        NINJA_ARCHIVE="ninja-linux.zip"
        NINJA_SHA256=5749cbc4e668273514150a80e387a957f933c6ed3f5f11e03fb30955e2bbead6
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
                CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-windows-x86_64.zip"
                CMAKE_SHA256=4d52ebab7193a698651639ed80d8d04fd903358843572cf44c7fd234cb7c26ab
                NINJA_ARCHIVE="ninja-win.zip"
                NINJA_SHA256=07fc8261b42b20e71d1720b39068c2e14ffcee6396b76fb7a795fb460b78dc65
                QT_HOST=windows
                QT_ARCH=win64_msvc2022_64
                QT_DIR=msvc2022_64
                ;;
            arm64)
                CMAKE_ARCHIVE="cmake-${CMAKE_VERSION}-windows-arm64.zip"
                CMAKE_SHA256=7b410ddd00e24c7250eec7452da2348a4a70437aa87e9cda0a20d6a85662fcff
                NINJA_ARCHIVE="ninja-winarm64.zip"
                NINJA_SHA256=e52f0bdef9dfb1003229dbd6508a508c4073fd017247002adc66e5e806cb0391
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

CMAKE_URL="https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/${CMAKE_ARCHIVE}"
CMAKE_ROOT="$PWD/.tools/cmake-${CMAKE_VERSION}"
NINJA_URL="https://github.com/ninja-build/ninja/releases/download/v${NINJA_VERSION}/${NINJA_ARCHIVE}"
NINJA_ROOT="$PWD/.tools/ninja-${NINJA_VERSION}"
QT_ROOT="$PWD/.tools/Qt/$QT_VERSION/$QT_DIR"
AQT_ROOT="$PWD/.tools/aqt-${AQTINSTALL_COMMIT:0:12}"
DOWNLOAD_ROOT="$PWD/.tools/downloads"

mkdir -p "$DOWNLOAD_ROOT"

# Downloads $2 to the cache unless the cached copy matches digest $3, and
# prints its path.
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

# Unpacks an archive into an empty directory, dropping one leading folder
# level when asked.
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
    [ -x "$CMAKE_ROOT/bin/cmake$EXE" ] &&
        [ -x "$CMAKE_ROOT/bin/ctest$EXE" ] &&
        [ "$("$CMAKE_ROOT/bin/cmake$EXE" --version | awk 'NR == 1 { print $3 }' | tr -d '\r')" = "$CMAKE_VERSION" ]
}

if ! cmake_ready; then
    archive=$(download "$CMAKE_ARCHIVE" "$CMAKE_URL" "$CMAKE_SHA256")
    stage="$PWD/.tools/cmake-${CMAKE_VERSION}.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    unpack "$archive" "$stage" 1
    if [ ! -x "$stage/bin/cmake$EXE" ] ||
        [ ! -x "$stage/bin/ctest$EXE" ] ||
        [ "$("$stage/bin/cmake$EXE" --version | awk 'NR == 1 { print $3 }' | tr -d '\r')" != "$CMAKE_VERSION" ]; then
        echo "CMake install verification failed" >&2
        exit 1
    fi
    rm -rf "$CMAKE_ROOT"
    mv "$stage" "$CMAKE_ROOT"
fi

ninja_ready() {
    [ -x "$NINJA_ROOT/ninja$EXE" ] &&
        [ "$("$NINJA_ROOT/ninja$EXE" --version | tr -d '\r')" = "$NINJA_VERSION" ]
}

if ! ninja_ready; then
    archive=$(download "ninja-${NINJA_VERSION}-${NINJA_ARCHIVE#ninja-}" "$NINJA_URL" "$NINJA_SHA256")
    stage="$PWD/.tools/ninja-${NINJA_VERSION}.stage.$$"
    rm -rf "$stage"
    mkdir -p "$stage"
    unpack "$archive" "$stage" 0
    chmod +x "$stage/ninja$EXE"
    if [ "$("$stage/ninja$EXE" --version | tr -d '\r')" != "$NINJA_VERSION" ]; then
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
