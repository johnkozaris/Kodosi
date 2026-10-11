#!/usr/bin/env bash
# Sourced by the Windows recipes, which run in Git Bash.

# The machine's native architecture: x64 or arm64. Git Bash itself may run
# emulated, so ask the native cmd.exe.
kodosi_windows_arch() {
    case "$(cmd //c 'echo %PROCESSOR_ARCHITECTURE%' | tr -d '\r')" in
        AMD64) echo x64 ;;
        ARM64) echo arm64 ;;
        *) echo "Kodosi builds on x64 or ARM64 Windows only" >&2; return 1 ;;
    esac
}

# Imports the newest Visual Studio C++ environment for this machine, so
# clang-cl, link and the Windows SDK work under Ninja.
kodosi_msvc_env() {
    local architecture tools vswhere install script name value
    architecture=$(kodosi_windows_arch) || return 1
    case "$architecture" in
        x64) tools=Microsoft.VisualStudio.Component.VC.Tools.x86.x64 ;;
        arm64) tools=Microsoft.VisualStudio.Component.VC.Tools.ARM64 ;;
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
        echo "No Visual Studio installation has the $architecture C++ build tools and C++ Clang Compiler for Windows" >&2
        return 1
    fi
    script=$(mktemp --suffix=.bat)
    printf '@call "%s\\VC\\Auxiliary\\Build\\vcvarsall.bat" %s >nul || exit /b 1\r\n@set\r\n' \
        "$(cygpath -w "$install")" "$architecture" > "$script"
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
    done < <(cmd //c "$(cygpath -w "$script")" | tr -d '\r')
    rm -f "$script"
    command -v clang-cl.exe >/dev/null || {
        echo "The Visual Studio environment did not provide clang-cl.exe" >&2
        return 1
    }
}
