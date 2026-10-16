# Kodosi for macOS

The native Swift app. It embeds the Rust runtime and Ghostty.

## Build and run

You need Apple silicon with macOS 26.4 or later
([Config/macOSDeploymentTarget](Config/macOSDeploymentTarget)), Xcode with the matching
SDK, the pinned Rust toolchain, and `just`, Python 3, `xcodegen`, `swiftformat` and
`swiftlint`.

```sh
just mac-build
```

This generates `KodosiDesktop.xcodeproj` from `project.yml`, builds the runtime library
and the app, and tests the shared package; run the `KodosiDesktop` scheme in Xcode.
Debug builds use a [local backend](../../docs/DEVELOPMENT.md#run-the-backend) at
`127.0.0.1:5180`; `just release-build` here builds against the public service.

## Command line

The app executable is also the `kodosi` CLI, with the app's Keychain identity. An app
installed in Applications links `~/.local/bin/kodosi` to itself (the Homebrew cask makes
its own link), and its own terminals always find it. For a build from source:

```sh
ln -s /absolute/path/Kodosi.app/Contents/MacOS/Kodosi ~/.local/bin/kodosi
```

## Updates

Release builds update with [Sparkle](https://sparkle-project.org) from
`https://updates.kodosi.com/macos/appcast.xml`, checked with the public key
`KODOSI_UPDATE_KEY` in `Config/Release.xcconfig`. `scripts/release/generate-appcast.sh`
signs the feed.

## Check

`just mac-check` from the repository root lints, tests and builds the Release app; here,
`just lint`, `just test` and `just format` do parts of it.
