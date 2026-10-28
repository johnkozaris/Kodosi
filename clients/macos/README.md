# Kodosi for macOS

This is the native Swift app. It embeds the Rust runtime and Ghostty.

## Build and run

You need Apple silicon with macOS 26.4 or later, Xcode with the matching SDK, the pinned
Rust toolchain, and `just`, Python 3, `xcodegen`, `swiftformat` and `swiftlint`.

```sh
just mac-build
```

Then run the `KodosiDesktop` scheme in Xcode. Debug builds use a
[local backend](../../docs/DEVELOPMENT.md#run-the-backend) at `127.0.0.1:5180`;
`just release-build` here builds against the public service. `just mac-check` from the
repository root lints, tests and builds the Release app; here, `just lint`, `just test`
and `just format` do parts of it.

## Command line

The app executable is also the `kodosi` CLI, with the app's Keychain identity. An app
installed in Applications links `~/.local/bin/kodosi` to itself (the Homebrew cask makes
its own link), and its own terminals always find it. For a build from source:

```sh
ln -s /absolute/path/Kodosi.app/Contents/MacOS/Kodosi ~/.local/bin/kodosi
```

## Releases

Release builds update themselves with [Sparkle](https://sparkle-project.org);
`Config/Release.xcconfig` sets the feed and its public key. Here, `just release-package`
makes the notarized disk image, `just cask-gen` the Homebrew cask, and
`scripts/release/generate-appcast.sh` the signed update feed.
