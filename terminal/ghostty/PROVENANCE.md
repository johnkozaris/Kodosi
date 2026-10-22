# Provenance and updates

The integration began from `Lakr233/libghostty-spm` tag `1.3.1` (commit
`b0930320739324886590e865d571eb5dd7073912`); the UIKit view adapts that commit's iOS
code. Keep its MIT attribution and Ghostty's license. This is not an official Ghostty
distribution.

## Pins

`MacOSGhostty.ref`, `LinuxGhostty.ref` and `WindowsGhostty.ref` pick the upstream commit
of each platform; iOS uses the macOS one. Root `Ghostty.lock` pins those commits and the
archive digests of the artifacts in `Vendor/`, and each artifact's `.version` file
records its inputs and outputs. The build options are in `NativeBuild.env` (Apple),
`LinuxVtBuild.env` and `WindowsVtBuild.env`. `Toolchain.env` pins Zig, and
`scripts/install-zig.sh` applies `Patches/zig` to it. `VERSION` names this package, not
the Ghostty version.

`just ghostty-check` checks the pins, digests and notices without a build. The Windows
Ghostty library workflow rebuilds the Windows x64 artifact from a clean checkout and
compares its record and headers with the committed ones.

## Update Ghostty

1. Rebase `Patches/ghostty` on the new upstream commit and drop the patches that
   upstream already contains.
2. Run `./scripts/build.sh` on each platform; for Windows x64, commit the artifact of
   the Windows Ghostty library workflow.
3. Update the third-party notices from the real source and artifacts.
4. If a VT header changed or the runtime needs a new symbol, add it to the allowlist of
   `runtime/crates/ghostty-vt-sys/generate-bindings.sh` and run the script on each
   platform; it needs network access, libclang and rustfmt.
5. Update `Ghostty.lock` from the `.version` files, run `./scripts/test.sh`,
   `just ghostty-check` and `just rust-check`, and try terminals in the apps.
6. Commit the patches, artifacts, notices, bindings and their users together.

Keep the licenses, corresponding source and relinking materials that the artifacts
require.
