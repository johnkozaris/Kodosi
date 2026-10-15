# Kodosi for macOS

The native Swift app. It embeds the Rust runtime and Ghostty.

## Requirements

Apple silicon with macOS 26.4 or later ([Config/macOSDeploymentTarget](Config/macOSDeploymentTarget)),
Xcode with the matching SDK, the pinned Rust toolchain, and `just`, Python 3, `xcodegen`,
`swiftformat` and `swiftlint` on your PATH.

## Build and run

```sh
just mac-build
```

This generates `KodosiDesktop.xcodeproj` from `project.yml`, builds the runtime library,
tests the shared package and builds the app. Run the `KodosiDesktop` scheme in Xcode.
Debug builds use a [local backend](../../docs/DEVELOPMENT.md#run-the-backend) at
`127.0.0.1:5180`; `just release-build` here builds against the public service.
[VERSION](../../VERSION) gives the version of all apps and the CLI.

## Command line

The app executable is also the `kodosi` CLI, using the app's Keychain identity. The
app's own terminals find it. For other terminals, Settings > General links
`/usr/local/bin/kodosi` to it, and the Homebrew cask makes the same link. For a build
from source, link it yourself:

```sh
ln -s /absolute/path/Kodosi.app/Contents/MacOS/Kodosi ~/.local/bin/kodosi
```

Keep the app running while you use room commands for its terminals.

## Updates

Release builds update with [Sparkle](https://sparkle-project.org) from
`https://updates.kodosi.com/macos/appcast.xml`; Debug and working-tree builds do not.
`KODOSI_UPDATE_KEY` in `Config/Release.xcconfig` is the public EdDSA key of the feed.

## Check

`just mac-check` from the repository root lints, tests and builds the Release app. Here,
`just lint`, `just test` and `just format` do parts of it, and `just working-tree-build`
builds an unpublishable archive of the working tree.
