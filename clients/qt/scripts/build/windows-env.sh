#!/usr/bin/env bash
# Sourced by the Windows recipes, which run in Git Bash.

# The machine's native architecture: x64 or arm64. Git Bash may run emulated
# and see AMD64 in its own environment, so read the system's value.
kodosi_windows_arch() {
    case "$(reg query 'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' //v PROCESSOR_ARCHITECTURE |
        awk '$1 == "PROCESSOR_ARCHITECTURE" { print $3 }' | tr -d '\r')" in
        AMD64) echo x64 ;;
        ARM64) echo arm64 ;;
        *) echo "Kodosi builds on x64 or ARM64 Windows only" >&2; return 1 ;;
    esac
}

# Exports this machine's KODOSI_WINDOWS_ARCH with its Qt build and Rust target.
kodosi_windows_target() {
    KODOSI_WINDOWS_ARCH=$(kodosi_windows_arch) || return 1
    case "$KODOSI_WINDOWS_ARCH" in
        x64) KODOSI_QT_ARCH=msvc2022_64 KODOSI_RUST_TARGET=x86_64-pc-windows-msvc ;;
        arm64) KODOSI_QT_ARCH=msvc2022_arm64 KODOSI_RUST_TARGET=aarch64-pc-windows-msvc ;;
    esac
    export KODOSI_WINDOWS_ARCH KODOSI_QT_ARCH KODOSI_RUST_TARGET
}

# Imports the newest Visual Studio C++ environment for this machine, so
# clang-cl, link and the Windows SDK tools work, after kodosi_windows_target.
kodosi_windows_env() {
    local tools llvm vswhere install script environment name value
    kodosi_windows_target || return 1
    case "$KODOSI_WINDOWS_ARCH" in
        x64) tools=Microsoft.VisualStudio.Component.VC.Tools.x86.x64 llvm=x64 ;;
        arm64) tools=Microsoft.VisualStudio.Component.VC.Tools.ARM64 llvm=ARM64 ;;
    esac
    vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
    if [ ! -x "$vswhere" ]; then
        echo "Install Visual Studio Build Tools with the C++ workload and its Clang tools" >&2
        return 1
    fi
    install=$("$vswhere" -latest -products '*' \
        -requires "$tools" Microsoft.VisualStudio.Component.VC.Llvm.Clang \
        -property installationPath | tr -d '\r')
    if [ -z "$install" ]; then
        echo "No Visual Studio installation has the $KODOSI_WINDOWS_ARCH C++ build tools and C++ Clang Compiler for Windows" >&2
        return 1
    fi
    script=$(mktemp --suffix=.bat)
    printf '@call "%s\\VC\\Auxiliary\\Build\\vcvarsall.bat" %s >nul || exit /b 1\r\n@set\r\n' \
        "$(cygpath -w "$install")" "$KODOSI_WINDOWS_ARCH" > "$script"
    environment=$(cmd //c "$(cygpath -w "$script")") || {
        rm -f "$script"
        echo "vcvarsall.bat $KODOSI_WINDOWS_ARCH failed in $install" >&2
        return 1
    }
    rm -f "$script"
    environment=${environment//$'\r'/}
    while IFS='=' read -r name value; do
        case "$name" in
            INCLUDE | LIB | LIBPATH | VCINSTALLDIR | VCToolsInstallDir | VCToolsVersion | VSINSTALLDIR \
                | WindowsSdkDir | WindowsSDKVersion | WindowsSdkBinPath | UniversalCRTSdkDir | UCRTVersion)
                export "$name=$value"
                ;;
            Path | PATH)
                PATH="$(cygpath -u -p "$value"):$PATH"
                export PATH
                ;;
        esac
    done <<< "$environment"
    # The installation's own Clang, which matches its C++ library, ahead of any
    # other LLVM on PATH.
    llvm="$(cygpath -u "$install")/VC/Tools/Llvm/$llvm/bin"
    if [ -d "$llvm" ]; then
        PATH="$llvm:$PATH"
    fi
    command -v clang-cl.exe >/dev/null || {
        echo "The Visual Studio environment did not provide clang-cl.exe" >&2
        return 1
    }
}
