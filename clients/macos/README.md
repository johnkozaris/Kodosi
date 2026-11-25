# Kodosi for macOS

Native Swift app embedding the Rust runtime and Ghostty.

## Build and run

In addition to the [shared toolchains](../../docs/DEVELOPMENT.md#toolchains), you need an Apple
silicon Mac running macOS 26.6+, Xcode 27.2+, and the pinned Rust toolchain. The first build
downloads the pinned SwiftLint, SwiftFormat, and XcodeGen versions (defined in
[Tools.env](../apple/Tools.env)) into `clients/apple/.tools/`.

```sh
just mac-build
```

Then open and run the `KodosiDesktop` scheme in Xcode. Debug builds connect to a
[local backend](../../docs/DEVELOPMENT.md#run-the-backend) at `127.0.0.1:5180` and authenticate
against the disposable Keycloak from `sign-in/keycloak.sh` at `localhost:8284` (override via
`KODOSI__AUTH__ISSUER`). Run `just release-build` inside `clients/macos/` to build against the
production service.

From the repo root, `just mac-check` lints, tests, and builds the Release app. Inside
`clients/macos/`, `just lint`, `just test`, and `just format` run individual steps.

## CLI integration

The app binary also acts as the `kodosi` CLI, sharing the app's Keychain identity. Installing the
app into `/Applications` symlinks `~/.local/bin/kodosi` automatically (the Homebrew cask creates
its own link), and shells inside Kodosi always find it on `PATH`. For a local build from source:

```sh
ln -s /absolute/path/Kodosi.app/Contents/MacOS/Kodosi ~/.local/bin/kodosi
```

## Releases

Release builds auto-update via [Sparkle](https://sparkle-project.org), configured with the feed
URL and public key in `Config/Release.xcconfig`. Inside `clients/macos/`, `just release-package`
builds the notarized DMG, `just cask-gen` updates the Homebrew cask, and
`scripts/release/generate-appcast.sh` generates the signed Sparkle appcast.
