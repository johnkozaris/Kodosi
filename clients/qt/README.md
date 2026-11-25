# Kodosi for Linux and Windows

Native Qt desktop app embedding the Rust runtime and Ghostty's VT engine. Build each platform
natively on that OS.

## Linux

Requires Linux x86-64 (Ubuntu 24.04 recommended), GCC 14+, the pinned Rust toolchain, `uv`,
`just`, an active desktop D-Bus session, and these system packages:

```sh
sudo apt-get install build-essential g++-14 git curl unzip zstd pkg-config \
  libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev \
  libglib2.0-dev libdbus-1-3 dbus-x11 libxcb-cursor0 libxkbcommon-x11-0 \
  libnss3 libasound2t64 libgbm1 libxcb-dri3-0 libxcomposite1 libxdamage1 \
  libxrandr2 libxtst6 libxkbfile1 libwayland-dev libxext-dev
export CC=gcc-14 CXX=g++-14
just linux-build
./clients/qt/build/dev/src/kodosi-qt
```

The first build downloads pinned Qt (with Qt WebEngine), CMake, Ninja, and Python tools into
`clients/qt/.tools/`, and compiles the pinned `agent-browser` binary next to the app.

`just linux-check` builds, tests, and lints. Inside `clients/qt/`, `just release-package` builds
the Debian, Arch, and portable packages into `build/release/` (requires `patchelf`, `dpkg-deb`,
`binutils`, `zstd`, `desktop-file-utils`, and `appstream`). The `.deb` package targets glibc 2.39+
and configures the Kodosi apt repository so OS updates update Kodosi too.

## Windows

Requires Windows 11 24H2+. Run all recipes from Git Bash with Visual Studio 2022+ (including the
"Desktop development with C++" workload and "C++ Clang Compiler for Windows"), Rust via `rustup`,
`just`, `uv` (`uv python install 3.12 --default`), and [NASM](https://www.nasm.us). Keep the
repository checkout path under roughly 90 characters to avoid Windows path-length limits.

```sh
just windows-check
```

Inside `clients/qt/`, `just windows-run` builds and launches the app, and `just windows-package`
outputs `build/release/kodosi-<version>-windows-<arch>.msix`. Install an unsigned test build from
an elevated PowerShell prompt:

```powershell
Add-AppxPackage -Path build\release\kodosi-<version>-windows-x64.msix -AllowUnsigned
```

The MSIX package puts `kodosi` on `PATH`, registers the `kodosi://` URL handler, stores data in
`%USERPROFILE%\.kodosi`, and pulls updates from its
[App Installer file](packaging/windows/Kodosi.appinstaller.in). Because Windows only applies MSIX
updates at launch, a running instance downloads the update and prompts you to restart. The repo
currently ships prebuilt Ghostty binaries for x64 only (built via the Windows Ghostty library
workflow), so Windows builds target x64 only.

## CLI

Release packages install `kodosi` alongside `kodosi-qt`. In a development checkout, run
`just rust-cli-build` to produce `./runtime/target/debug/kodosi`. Third-party notices live in
[packaging/licenses](packaging/licenses/).
