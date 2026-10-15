# Kodosi for Linux and Windows

The native Qt app. It embeds the Rust runtime and the Ghostty terminal engine. Build
each platform on that platform.

## Linux

You need Linux x86-64 (Ubuntu 24.04 recommended) with a C++23 compiler, the pinned Rust
toolchain, Python 3, `uv`, `just`, a desktop D-Bus session, and the development
libraries for OpenGL/EGL, XKB, Fontconfig and GLib. On Ubuntu:

```sh
sudo apt-get install build-essential git curl unzip zstd pkg-config \
  libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev \
  libglib2.0-dev libdbus-1-3 dbus-x11 libxcb-cursor0 libxkbcommon-x11-0
```

```sh
just linux-build
./clients/qt/build/dev/src/kodosi-qt
```

The first build installs the Qt, CMake, Ninja and Python tools pinned in
[dependencies.lock.json](dependencies.lock.json) under `.tools/`. `just linux-check`
builds, tests and lints. `just release-package` here makes Debian, Arch and portable
packages in `build/release/` from committed changes; it needs `patchelf`, `dpkg-deb`,
`binutils`, `zstd`, `desktop-file-utils` and `appstream`. The Debian package needs
glibc 2.39 or later.

## Windows

Kodosi for Windows runs on Windows 11, x64 or ARM64, with the pinned ConPTY host. Run the
recipes in Git Bash with Visual Studio 2022 or later (Desktop development with C++, C++
Clang Compiler for Windows, and the ARM64 build tools on ARM64), Rust through `rustup`,
`just`, `uv` (`uv python install 3.12 --default` for `python3`), and
[NASM](https://www.nasm.us) on x64. Keep the checkout path under about 90 characters:
the C compiler cannot write a path longer than 259 characters.

```sh
just windows-check
```

`just windows-run` here builds and starts the app; notifications need the installed
package. `just windows-package` makes `build/release/kodosi-<version>-windows-<arch>.msix`,
with the version from [CMakeLists.txt](CMakeLists.txt). Install it unsigned from an
administrator PowerShell:

```powershell
Add-AppxPackage -Path build\release\kodosi-<version>-windows-x64.msix -AllowUnsigned
```

The package adds `kodosi` to the PATH, opens `kodosi://` links, and keeps data in
`%USERPROFILE%\.kodosi`. The x64 Ghostty library comes from the Windows Ghostty library
workflow, as the [provenance guide](../../terminal/ghostty/PROVENANCE.md) says.

## Command line

Packages install `kodosi` beside `kodosi-qt`. From a checkout:

```sh
just rust-cli-build
./runtime/target/debug/kodosi --help
```

Add that folder to your PATH, and keep the app running while you use room commands for
its terminals. Third-party notices are in [packaging/licenses](packaging/licenses/).
