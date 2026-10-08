# Kodosi for macOS

The native macOS client lives in this repository alongside `../../runtime` and
`../../terminal/ghostty`.

Use Apple silicon, Xcode, the pinned Rust toolchain, `just`, `xcodegen`,
`swiftformat`, and `swiftlint`. Keep tools and dependencies project-local.
Xcode signing settings can be overridden with your own development team.

From the repository root:

```sh
just mac-build
just mac-check
```

The client directory also provides `just test`, `just build-release` and release
packaging commands. Official signing uses `KODOSI_TEAM_ID` and local signing
credentials; private keys and provisioning profiles must stay outside Git.
`just archive-working-tree` builds a non-publishable validation archive.

See [Ghostty integration](docs/GHOSTTY_INTEGRATION.md) before changing the native
renderer. Generate the Xcode project from `project.yml`; never edit generated
project files or the runtime header directly.
