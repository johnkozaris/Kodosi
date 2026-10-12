# Kodosi for Linux and Windows

The native Qt app embeds the shared Rust runtime and Ghostty terminal engine.
Everything builds from this checkout. Build each platform on that platform.

## Requirements

Use Linux x86-64 with a C++23 compiler, the pinned Rust toolchain, Python 3, `uv`,
`just`, and a desktop D-Bus session. Ubuntu 24.04 is the recommended starting point.

The build needs the system development libraries for OpenGL/EGL, XKB, Fontconfig,
and GLib, plus the runtime libraries required by Qt's X11 or Wayland plugins.
On Ubuntu, a starting set is:

```sh
sudo apt-get install build-essential git curl unzip zstd pkg-config \
  libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev \
  libglib2.0-dev libdbus-1-3 dbus-x11 libxcb-cursor0 libxkbcommon-x11-0
```

Install Rust, `uv`, and `just` if they are not already available. The recipe
`tools-install` installs the pinned Qt, CMake, Ninja and Python tools under `.tools/`
in this client directory. It does not require a separate system Qt installation.
[dependencies.lock.json](dependencies.lock.json) is the record of the versions and the
SHA-256 values of these tools. The recipe reads them from that file.

## Build and run

From the repository root:

```sh
just linux-build
./clients/qt/build/dev/src/kodosi-qt
```

The first build downloads the pinned tools. Build output stays under `build/`.
The native app starts as `kodosi-qt`; `kodosi` is its command-line companion.

Use [Getting started](../../docs/GETTING_STARTED.md) for terminals and rooms.

## Command-line access

For a source checkout, build the development CLI from the repository root:

```sh
just rust-cli-build
./runtime/target/debug/kodosi --help
```

Add that directory to your shell's PATH to use `kodosi` from agent terminals.
On Windows, the recipe also puts the ConPTY host beside `kodosi.exe`.
Keep the native app running when working with its room terminals. Packaged builds
install both `kodosi` and `kodosi-qt` launchers.

## Check and package

From the repository root:

```sh
just linux-check
```

From this client directory, `just configure`, `just test`, and `just lint` are
also available. `just release-package` makes a Debian package, a portable archive
and an Arch package in `build/release/`. Commit all the changes first. The recipe
needs `patchelf`, `dpkg-deb`, `binutils`, `zstd`, `desktop-file-utils` and
`appstream`.

The Debian package targets glibc 2.39 or later and the Ubuntu 24.04 library names.
Do not assume it installs on older distributions just because a source build works
there. The portable archive still requires a compatible Linux system and desktop
libraries.

Each package records the Kodosi commit. The release recipe checks the content and
the license files of each package, and writes a release manifest that a person can
sign. Kodosi has no public release of the app yet.

## Windows

Kodosi for Windows runs on Windows 11, x64 or ARM64. Its terminals use the pinned
ConPTY host, `conpty.dll` and `OpenConsole.exe`, which the build puts beside the app
and the `kodosi` command.

### Windows requirements

Run the recipes in Git Bash from [Git for Windows](https://gitforwindows.org), with:

- Visual Studio 2022 or later, or its Build Tools, with the *Desktop development
  with C++* workload and *C++ Clang Compiler for Windows*; on ARM64, also the ARM64
  build tools. The app builds with clang-cl against Qt's MSVC libraries.
- Rust through `rustup`, `just`, and `uv`. Run `uv python install 3.12 --default` so
  that `python3` exists.
- On x64, [NASM](https://www.nasm.us) on `PATH`; the runtime's TLS library needs it.

The recipe `tools-install` installs the pinned Qt, CMake and Ninja under `.tools/`, as
on Linux.

### Terminal engine

Each Windows architecture links its own Ghostty library, built on that architecture.
If `terminal/ghostty/Vendor/GhosttyVt/windows-<arch>` is not in the checkout, build it
from the repository root:

```sh
terminal/ghostty/scripts/build.sh
```

The script prints the line to add to [Ghostty.lock](../../Ghostty.lock), for example
`windows_x86_64_archive_sha256=<digest>`. Commit the library and the digest
together, as the [provenance guide](../../terminal/ghostty/PROVENANCE.md) describes.

### Build and check on Windows

From the repository root:

```sh
just windows-check
```

From this client directory, `just windows-run` builds the app and starts it with the
installed Qt libraries. On any host with Docker, `just windows-rust-lint` from the
repository root lints the runtime for both Windows targets. Desktop notifications need the installed
package; a development build does not show them.

### Package and sign

From this client directory, `just windows-package` builds, tests, and verifies
`build/release/kodosi-<version>-windows-<arch>.msix`. The `project` command in
[CMakeLists.txt](CMakeLists.txt) gives the version. The recipes and the release
scripts read the version from that file. The package holds the app, the
`kodosi` command, ConPTY, the C runtime, Qt, and the notices. Without
`KODOSI_MSIX_PUBLISHER`, the package is unsigned; install it from an administrator
PowerShell:

```powershell
Add-AppxPackage -Path build\release\kodosi-<version>-windows-x64.msix -AllowUnsigned
```

For a release, set `KODOSI_MSIX_PUBLISHER` to the subject of the Azure Artifact
Signing certificate before packaging. Install the signing client with
`winget install -e --id Microsoft.Azure.ArtifactSigningClientTools`, sign in to
Azure, and run:

```sh
KODOSI_SIGNING_DLIB=<path>/Azure.CodeSigning.Dlib.dll \
KODOSI_SIGNING_METADATA=<path>/metadata.json \
just windows-sign build/release/kodosi-<version>-windows-x64.msix
```

Publish the signed x64 and ARM64 packages to winget with `wingetcreate`.

The installed app adds the `kodosi` command to the user's PATH as an app execution
alias and opens `kodosi://` links. The app and its command share their data in
`%USERPROFILE%\.kodosi`.

See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[development setup](../../docs/DEVELOPMENT.md). Kodosi is MIT licensed; third-party
notices and relinking materials are under [packaging/licenses](packaging/licenses/).
