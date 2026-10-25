# Kodosi for Linux and Windows

The native Qt app. It embeds the Rust runtime and the Ghostty terminal engine. Build
each platform on that platform.

## Linux

You need Linux x86-64 (Ubuntu 24.04 recommended) with a C++23 compiler, the pinned Rust
toolchain, Python 3, `uv`, `just`, a desktop D-Bus session and these libraries:

```sh
sudo apt-get install build-essential git curl unzip zstd pkg-config \
  libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev \
  libglib2.0-dev libdbus-1-3 dbus-x11 libxcb-cursor0 libxkbcommon-x11-0
just linux-build
./clients/qt/build/dev/src/kodosi-qt
```

The first build puts the pinned Qt, CMake, Ninja and Python tools in `.tools/` here.
`just linux-check` builds, tests and lints. Here, `just release-package` makes the
Debian, Arch and portable packages in `build/release/`; it needs `patchelf`, `dpkg-deb`,
`binutils`, `zstd`, `desktop-file-utils` and `appstream`. The Debian package needs
glibc 2.39 or later and adds the Kodosi apt repository, so system updates update Kodosi.

## Windows

Kodosi runs on Windows 11 24H2 or later. Run the recipes in Git Bash, with Visual Studio
2022 or later (Desktop development with C++ and C++ Clang Compiler for Windows), Rust
through `rustup`, `just`, `uv` (`uv python install 3.12 --default`) and
[NASM](https://www.nasm.us). Keep the checkout path under about 90 characters.

```sh
just windows-check
```

Here, `just windows-run` builds and starts the app, and `just windows-package` makes
`build/release/kodosi-<version>-windows-<arch>.msix`. Install it unsigned from an
administrator PowerShell:

```powershell
Add-AppxPackage -Path build\release\kodosi-<version>-windows-x64.msix -AllowUnsigned
```

The package puts `kodosi` on the PATH, opens `kodosi://` links, keeps data in
`%USERPROFILE%\.kodosi`, and gets updates from its
[App Installer file](packaging/windows/Kodosi.appinstaller.in). Windows installs an
update only when Kodosi starts, so a running Kodosi downloads it and offers to restart.
The repository has the Ghostty library for x64 only, built by the Windows Ghostty
library workflow, so the app builds for x64 only.

## Command line

Packages install `kodosi` beside `kodosi-qt`. From a checkout, `just rust-cli-build`
builds `./runtime/target/debug/kodosi`. Third-party notices are in
[packaging/licenses](packaging/licenses/).
