# Kodosi for macOS

The native Swift app embeds the shared Rust runtime and Ghostty terminal integration.

## Requirements

- Apple silicon and macOS 26.4 or later. [Config/macOSDeploymentTarget](Config/macOSDeploymentTarget)
  is the deployment target authority.
- Xcode with the matching macOS SDK and command-line tools selected.
- The Rust toolchain pinned at the repository root.
- `just`, Python 3, `xcodegen`, `swiftformat`, and `swiftlint` on your build PATH.

Keep downloaded build tools and dependencies project-local.

## Build and run

From the repository root:

```sh
just mac-build
```

This generates `KodosiDesktop.xcodeproj`, builds the runtime library and the app, and
runs the terminal package tests. Open the generated project in Xcode, select the
`KodosiDesktop` scheme, and run it.

Debug builds target the local backend at `127.0.0.1:5180`; follow
[backend setup](../../docs/DEVELOPMENT.md#run-the-backend) to run it. Release builds use
the public service addresses:

```sh
just --justfile clients/macos/justfile release-build
```

Generate the project from `project.yml`; do not edit generated project files or the
runtime header directly. [VERSION](../../VERSION) gives the
version of the Mac app, the iOS app, the Linux and Windows app and the CLI.

## Command-line access

The app executable also implements the CLI:

```sh
/absolute/path/KodosiDesktop.app/Contents/MacOS/KodosiDesktop --help
```

For `kodosi` in your shells and agent terminals, put a symlink named `kodosi` on your
PATH pointing to that executable. Use the absolute path to your built app. The CLI
must be the signed app executable to use the app's Keychain identity.

Keep the app running when using room commands for its terminals. See
[Agents and tasks](../../docs/AGENTS_AND_TASKS.md).

## Check and package

```sh
just mac-check
```

From this client directory, `just lint` and `just test` do parts of this check. To
format the Swift sources, run `just format`.
`just working-tree-build` builds an archive of the working tree for validation.

Read [Ghostty integration](docs/GHOSTTY_INTEGRATION.md) before changing the renderer.
See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[contributor setup](../../docs/DEVELOPMENT.md).
