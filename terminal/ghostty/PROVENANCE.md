# Provenance and updates

Kodosi's integration began from `Lakr233/libghostty-spm` tag `1.3.1`, commit
`b0930320739324886590e865d571eb5dd7073912`; the UIKit view adapts that commit's
iOS implementation. Preserve the imported MIT attribution and Ghostty's license.
This is not an official Ghostty distribution.

## Source and artifacts

`MacOSGhostty.ref`, `LinuxGhostty.ref`, and `WindowsGhostty.ref` select independent
upstream commits. On Apple platforms the renderer and terminal engine are one native
image, built from `MacOSGhostty.ref` as `macos-arm64`, `ios-arm64` and
`ios-arm64-simulator` slices of `Vendor/GhosttyKit.xcframework`; Linux and each Windows
architecture have their own terminal-engine artifact. `Vendor/*.version` records the
inputs and measured output for each build. `VERSION` names the package, not the
upstream Ghostty version.

Root `../../Ghostty.lock` selects every platform artifact. The integration, runtime
and clients share the Kodosi repository revision. Preserve platform-specific
provenance even when refs choose the same upstream commit. Native artifact
metadata records measured build inputs and outputs, independent of repository layout.

The integration was imported from `johnkozaris/kodosi-ghostty` at commit
`086ea213b7d1f890c5e7f2569b1848c3cd75d3d3`; its upstream attributions, licenses,
corresponding source and relinking materials are retained.

## Rebuild

`./Script/build.sh` is the only native build entry point. It uses pinned local
tools and a clean upstream checkout, applies `Patches/ghostty`, builds and
verifies the artifacts, and records their actual digests. `--source` selects a
source cache, not a prepatched tree. Never hand-edit build evidence.

Patches support host-managed terminal I/O, checkpoint restore, native policy,
and the combined image, including the iOS slices that upstream otherwise limits
to `libghostty-vt`. Remove patches when upstream supplies the same behavior.
Do not introduce another process backend or terminal parser.

## Updates

Review the upstream changes, rebase patches, rebuild each platform, and update
notices from real source and artifact evidence. Run package tests, artifact
verifiers, and native sanitizer checks. Update root `Ghostty.lock` from the measured artifacts and commit the integration,
artifacts, notices and affected consumers together. Validate the actual
Rust, Swift, and Qt integration and terminal lifecycle.

Working-tree validation is non-publishable. Preserve licenses, corresponding
source, and relinking materials required by the built artifacts. Native builds
need not be byte-for-byte reproducible; record the inputs and committed output.
