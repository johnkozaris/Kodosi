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
[backend setup](../../docs/DEVELOPMENT.md#run-the-backend) to run it. For your own
service, build the Release configuration:

```sh
just --justfile clients/macos/justfile build-release
```

Run the Release app from Xcode's Products group. Release uses the public service
addresses, and that service is not open yet, so supply the
[self-hosting environment](../../docs/SELF-HOSTING.md).
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

From this client directory, `just lint` and `just test` do parts of this check. To
format the Swift sources, run `just format`.
`just archive-working-tree` builds a non-publishable validation archive.

Official releases are signed with Developer ID, embed a provisioning profile for the
Keychain access group, and are notarized by Apple. The signing credentials stay with
the maintainers, outside Git.

To make a release, set `KODOSI_TEAM_ID`, `KODOSI_MARKETING_VERSION` and
`KODOSI_BUILD_NUMBER`. Then run one of these scripts from this client directory:

| Script | Result |
| --- | --- |
| `scripts/release/export.sh` | The signed app from a clean checkout, in `build/export/` |
| `scripts/release/notarize.sh` | The result of `export.sh`, after Apple notarizes it |
| `scripts/release/package-dmg.sh` | The result of `notarize.sh`, and a signed and notarized `build/export/KodosiDesktop.dmg` |
| `scripts/release/generate-cask.sh` | A Homebrew cask in `build/KodosiDesktop.rb`, if the disk image at `KODOSI_DOWNLOAD_URL` is the same as the local one |

The scripts examine the signatures, the embedded profile, the entitlements, the native
artifacts and the source revision. `just verify-signed` examines the app in the archive.

Read [Ghostty integration](docs/GHOSTTY_INTEGRATION.md) before changing the renderer.
See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[contributor setup](../../docs/DEVELOPMENT.md).
