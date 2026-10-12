# Kodosi for macOS

The native Swift app embeds the shared Rust runtime and Ghostty terminal integration.
Everything builds from this checkout.

## Requirements

- Apple silicon and macOS 26.4 or later. [Config/macOSDeploymentTarget](Config/macOSDeploymentTarget)
  is the deployment target authority.
- Xcode with the matching macOS SDK and command-line tools selected.
- The Rust toolchain pinned at the repository root.
- `just`, Python 3, `xcodegen`, `swiftformat`, and `swiftlint` on your build PATH.

Keep downloaded build tools and dependencies project-local. To build and sign the app,
you need your own Apple development signing setup.

## Build and run

Put your Apple development team in `clients/apple/Config/Signing.local.xcconfig`, for
example `DEVELOPMENT_TEAM = ABCDE12345`. Git ignores this file.
[Apps.xcconfig](../apple/Config/Apps.xcconfig) includes it for the Mac app and the iOS
app.

From the repository root:

```sh
just mac-build
```

This generates `KodosiDesktop.xcodeproj`, builds the runtime and app, and runs the
terminal package tests. Open the generated project in Xcode, select the
`KodosiDesktop` scheme, and run it.

Debug builds target the local backend at `127.0.0.1:5180`; follow
[backend setup](../../docs/DEVELOPMENT.md#run-the-backend) to run it. For your own
service, build the Release configuration:

```sh
just --justfile clients/macos/justfile release-build
```

Run the Release app from Xcode's Products group. Release uses the public service
addresses, and that service is not open yet, so supply the
[self-hosting environment](../../docs/SELF-HOSTING.md).
For a custom server in Debug, clear the two `KODOSI_DEBUG_BACKEND_*` build settings
so its local override does not replace your environment.

Generate the project from `project.yml`; do not edit generated project files or the
runtime header directly. [Apps.xcconfig](../apple/Config/Apps.xcconfig) gives the
version and the build number of the Mac app and of the iOS app.

## Command-line access

The app executable also implements the CLI:

```sh
/absolute/path/KodosiDesktop.app/Contents/MacOS/KodosiDesktop --help
```

For `kodosi` in your shells and agent terminals, put a symlink named `kodosi` on your
PATH pointing to that executable. Use the absolute path to your built app. It must
remain the signed app executable so the CLI uses the same Keychain identity; a
separate unsigned Rust binary is not a substitute for this app's CLI.

Keep the app running when using room commands for its terminals. See
[Agents and tasks](../../docs/AGENTS_AND_TASKS.md).

## Check and package

```sh
just mac-check
```

From this client directory, `just lint` and `just test` do parts of this check. To
format the Swift sources, run `just format`.
`just working-tree-build` builds an archive of the working tree for validation. You
cannot publish it.

Official releases have a Developer ID signature and Apple notarization, and the signing
credentials and release values stay with the maintainers, outside Git.

Read [Ghostty integration](docs/GHOSTTY_INTEGRATION.md) before changing the renderer.
See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[contributor setup](../../docs/DEVELOPMENT.md).
