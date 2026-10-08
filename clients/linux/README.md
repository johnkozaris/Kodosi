# Kodosi for Linux

The native Qt app embeds the shared Rust runtime and Ghostty terminal engine.
Everything builds from this checkout.

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

Install Rust, `uv`, and `just` if they are not already available. The repository's
bootstrap installs its pinned Qt, CMake, Ninja, and Python tooling under `.tools/`
in this client directory. It does not require a separate system Qt installation.

## Build and run

From the repository root:

```sh
just linux-build
./clients/linux/build/dev/src/kodosi-qt
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
Keep the native app running when working with its room terminals. Packaged builds
install both `kodosi` and `kodosi-qt` launchers.

## Check and package

From the repository root:

```sh
just linux-check
```

From this client directory, `just configure`, `just test`, and `just lint` are
also available. `just package` builds and verifies a Debian package and portable
archive under `build/release/`; packaging additionally needs tools such as
`patchelf`, `dpkg-deb`, and `binutils`.

The Debian package targets glibc 2.39 or later and the Ubuntu 24.04 library names.
Do not assume it installs on older distributions just because a source build works
there. The portable archive still requires a compatible Linux system and desktop
libraries.

Release recipes record the monorepo revision, verify package contents and license
materials, and can create a signed release manifest. Public app releases are not
published yet.

See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[development setup](../../docs/DEVELOPMENT.md). Kodosi is MIT licensed; third-party
notices and relinking materials are under [packaging/licenses](packaging/licenses/).
