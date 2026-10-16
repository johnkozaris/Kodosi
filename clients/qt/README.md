# Kodosi for Linux and Windows

The native Qt app. It embeds the Rust runtime and the Ghostty terminal engine. Build
each platform on that platform.

## Linux

You need Linux x86-64 (Ubuntu 24.04 recommended) with a C++23 compiler, the pinned Rust
toolchain, Python 3, `uv`, `just`, a desktop D-Bus session and these development
libraries:

```sh
sudo apt-get install build-essential git curl unzip zstd pkg-config \
  libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev \
  libglib2.0-dev libdbus-1-3 dbus-x11 libxcb-cursor0 libxkbcommon-x11-0
just linux-build
./clients/qt/build/dev/src/kodosi-qt
```

The first build installs the Qt, CMake, Ninja and Python tools pinned in
[dependencies.lock.json](dependencies.lock.json) under `.tools/`. `just linux-check`
builds, tests and lints. `just release-package` here makes Debian, Arch and portable
packages in `build/release/` (it needs `patchelf`, `dpkg-deb`, `binutils`, `zstd`,
`desktop-file-utils` and `appstream`). The Debian package needs glibc 2.39 or later and
adds the Kodosi apt repository, so system updates update Kodosi.

## Windows

Kodosi runs on Windows 11 24H2 or later, x64 or ARM64, with the pinned ConPTY host. Run
the recipes in Git Bash with Visual Studio 2022 or later (Desktop development with C++,
C++ Clang Compiler for Windows, and the ARM64 build tools on ARM64), Rust through
`rustup`, `just`, `uv` (`uv python install 3.12 --default`), and
[NASM](https://www.nasm.us) on x64. Keep the checkout path under about 90 characters.

```sh
just windows-check
```

Here, `just windows-run` builds and starts the app, and `just windows-package` makes
`build/release/kodosi-<version>-windows-<arch>.msix`. Install it unsigned from an
administrator PowerShell:

```powershell
Add-AppxPackage -Path build\release\kodosi-<version>-windows-x64.msix -AllowUnsigned
```

The package adds `kodosi` to the PATH, opens `kodosi://` links, keeps data in
`%USERPROFILE%\.kodosi`, and updates itself from its
[App Installer file](packaging/windows/Kodosi.appinstaller.in) when Kodosi is not in use.
`just windows-bundle` makes the release bundle. The x64 Ghostty library comes from the
Windows Ghostty workflow; there is no ARM64 library yet, so no ARM64 package.

## Command line

Packages install `kodosi` beside `kodosi-qt`. From a checkout, `just rust-cli-build`
builds `./runtime/target/debug/kodosi`. Third-party notices are in
[packaging/licenses](packaging/licenses/).
