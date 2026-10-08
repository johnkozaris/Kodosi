# Kodosi for macOS

The native Swift app embeds the shared Rust runtime and Ghostty terminal integration.
Everything builds from this checkout.

## Requirements

- Apple silicon and macOS 26.4 or later. [Config/macOSDeploymentTarget](Config/macOSDeploymentTarget)
  is the deployment target authority.
- Xcode with the matching macOS SDK and command-line tools selected.
- The Rust toolchain pinned at the repository root.
- `just`, Python 3, `xcodegen`, `swiftformat`, and `swiftlint` on your build PATH.

Keep downloaded build tools and dependencies project-local. Building and signing the
app requires your own Apple development signing setup. The checked-in team identifier
is public configuration for the official app, not a signing credential.

## Build and run

For your own signing setup, set `DEVELOPMENT_TEAM` in the `appBase` settings of
[project.yml](project.yml). Keep personal signing changes out of unrelated pull requests.

From the repository root:

```sh
just mac-build
```

This generates `KodosiDesktop.xcodeproj`, builds the runtime and app, and runs the
terminal package tests. Open the generated project in Xcode, select the
`KodosiDesktop` scheme, and run it.

Debug builds target the local backend at `127.0.0.1:5180`; follow
[backend setup](../../docs/DEVELOPMENT.md#run-the-backend) to run it. To try the hosted
service, build the Release configuration:

```sh
just --justfile clients/macos/Justfile build-release
```

Run the Release app from Xcode's Products group. Release uses the public service
defaults unless you supply the [self-hosting environment](../../docs/SELF-HOSTING.md).
For a custom server in Debug, clear the two `KODOSI_DEBUG_BACKEND_*` build settings
so its local override does not replace your environment.

Generate the project from `project.yml`; do not edit generated project files or the
runtime header directly.

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

The client directory also provides `just lint`, `just test`, and release recipes.
`just archive-working-tree` builds a non-publishable validation archive.

Official distribution uses Developer ID signing, a provisioning profile for the
Keychain access group, and notarization. `KODOSI_TEAM_ID` selects the release team;
credentials, private keys, and provisioning profiles stay outside Git. The release
scripts verify signatures, entitlements, native artifacts, and source provenance.

Read [Ghostty integration](docs/GHOSTTY_INTEGRATION.md) before changing the renderer.
See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[contributor setup](../../docs/DEVELOPMENT.md).
