# Provenance and updates

Kodosi's integration began from `Lakr233/libghostty-spm` tag `1.3.1`, commit
`b0930320739324886590e865d571eb5dd7073912`; the UIKit view adapts that commit's
iOS implementation. Preserve the imported MIT attribution and Ghostty's license.
This is not an official Ghostty distribution.

## Source and artifacts

`MacOSGhostty.ref`, `LinuxGhostty.ref`, and `WindowsGhostty.ref` select independent
upstream commits. On Apple platforms the renderer and terminal engine are one native
image, built from `MacOSGhostty.ref` as one slice of `Vendor/GhosttyKit.xcframework`
for each Apple slice in `NativeBuild.env`; Linux and each Windows
architecture have their own terminal-engine artifact. `Vendor/*.version` records the
inputs and measured output for each build. `VERSION` names the package, not the
upstream Ghostty version.

Root `../../Ghostty.lock` selects every platform artifact. The integration, runtime
and clients share the Kodosi repository revision. Preserve platform-specific
provenance even when refs choose the same upstream commit. Native artifact
metadata records measured build inputs and outputs, independent of repository layout.

`Ghostty.lock` has two kinds of keys:

- `<platform>_ghostty_commit` is the commit in the ref file of a platform, for
  example `macos_ghostty_commit` for `MacOSGhostty.ref`. The iOS slices use the
  macOS ref.
- `<artifact>_archive_sha256` is the SHA-256 of the archive of one artifact. The
  artifact is an Apple slice in `NativeBuild.env` or a VT layout, for example
  `ios_arm64_simulator_archive_sha256` for `ios-arm64-simulator`. Write underscores
  in place of the hyphens of the name.

The integration was imported from `johnkozaris/kodosi-ghostty` at commit
`086ea213b7d1f890c5e7f2569b1848c3cd75d3d3`; its upstream attributions, licenses,
corresponding source and relinking materials are retained.

### Build values

`NativeBuild.env` is the only record of the Apple build values:

- `GHOSTTY_BUILD_APPLE_SLICES` lists the Apple slices. A slice name is
  `<platform>-arm64`, or `<platform>-arm64-simulator` for a simulator.
- Each platform has a Zig target and an i18n option, for example
  `GHOSTTY_BUILD_IOS_TARGET` and `GHOSTTY_BUILD_IOS_I18N`. A simulator slice adds
  `-simulator` to the Zig target of its platform.

The build scripts, the verifiers and the Rust build script read these values from
`NativeBuild.env`. Do not copy the slice list or a target into a script.

The iOS version in `GHOSTTY_BUILD_IOS_TARGET` must not be higher than the iOS
deployment target of the app in `../../clients/apple/Config/iOSDeploymentTarget`.
`Package.swift` must give the same iOS version as `GHOSTTY_BUILD_IOS_TARGET`.
`just mac-check` and `just ios-check` make sure of these two conditions.

The notice verifier reads the z2d source identity from
`ThirdPartyNotices/inventory.json`. It gets the list of libintl objects from the
compiled sources in the libintl corresponding-source manifest. The libintl relink
script uses the same list.

## Rebuild

`./scripts/build.sh` is the only native build entry point. It uses pinned local
tools and a clean upstream checkout, applies `Patches/ghostty`, builds and
verifies the artifacts, and records their actual digests. `--source` selects a
source cache, not a prepatched tree. Never hand-edit build evidence.

Patches support host-managed terminal I/O, checkpoint restore, native policy,
and the combined image, including the iOS slices that upstream otherwise limits
to `libghostty-vt`. Remove patches when upstream supplies the same behavior.
Do not introduce another process backend or terminal parser.

## Updates

Do these steps to update Ghostty:

1. Examine the upstream changes. Rebase the patches in `Patches/ghostty`.
2. Rebuild each platform with `./scripts/build.sh` on a host of that platform.
3. Update the notices from the real source and artifact evidence.
4. If a rebuild changes a VT header, generate the Rust bindings again. See
   [Rust bindings](#rust-bindings).
5. Run the package tests, the artifact verifiers and the native sanitizer checks.
6. Update root `Ghostty.lock` from the measured artifacts.
7. Run `just ghostty-check` from the repository root.
8. Do a test of the real Rust, Swift and Qt integration and of the terminal lifecycle.
9. Commit the integration, the artifacts, the notices, the bindings and the affected
   consumers together.

Working-tree validation is non-publishable. Preserve licenses, corresponding
source, and relinking materials required by the built artifacts. Native builds
need not be byte-for-byte reproducible; record the inputs and committed output.

### Rust bindings

The crate `runtime/crates/ghostty-vt-sys` compiles Rust bindings that are in Git.
There is one file for each platform: `src/bindings_macos.rs` for macOS and iOS,
`src/bindings_linux.rs` and `src/bindings_windows.rs`. The script
`generate-bindings.sh` in the crate writes these files from the VT headers in
`Vendor/GhosttyVt`.

Generate the bindings again when one of these conditions occurs:

- A rebuild changes a VT header.
- The runtime needs a Ghostty function, type or constant that the bindings do not
  have. Add it to the allowlist in the script first.

Run this command from the repository root, on a host of the platform, after the build
of its artifact:

```sh
runtime/crates/ghostty-vt-sys/generate-bindings.sh
```

The script finds the platform of the host. To select a platform, give `macos-arm64`,
`linux-x86_64`, `windows-x86_64` or `windows-arm64` as the argument. The script
installs the pinned `bindgen-cli` in a temporary folder. It needs network access,
libclang and rustfmt. Then run `just rust-check`. Commit the changed bindings with
the artifacts.
