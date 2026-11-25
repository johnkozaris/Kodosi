# Ghostty provenance and updates

This integration originated from `Lakr233/libghostty-spm` tag `1.3.1` (commit
`b0930320739324886590e865d571eb5dd7073912`), and the UIKit view adapts that commit's iOS code.
Preserve its MIT attribution alongside Ghostty's license. This is not an official Ghostty
distribution.

## Version pins

- `MacOSGhostty.ref`, `LinuxGhostty.ref`, `WindowsGhostty.ref`, and `AndroidGhostty.ref` pin the
  upstream Ghostty commit per platform (iOS shares the macOS ref).
- Root `Ghostty.lock` records those commits and the archive digests of prebuilt artifacts in
  `Vendor/`, while each artifact's `.version` file logs its exact build inputs and outputs.
- Build flags live in `NativeBuild.env` (Apple), `LinuxVtBuild.env`, `WindowsVtBuild.env`, and
  `AndroidVtBuild.env` (which also pins the Android NDK).
- `Toolchain.env` pins Zig, and `scripts/install-zig.sh` applies `Patches/zig`.
- `VERSION` tracks this wrapper package version, not Ghostty's version.

Run `just ghostty-check` to verify pins, digests, and notices without rebuilding. The Windows
Ghostty library workflow rebuilds the Windows x64 artifact from a clean checkout and compares its
manifest and headers against the committed copy.

## Updating Ghostty

1. Rebase `Patches/ghostty` onto the new upstream commit and drop any patches merged upstream.
2. Run `./scripts/build.sh` on each host platform and `./scripts/build.sh --android` on macOS or
   Linux with the pinned NDK. For Windows x64, commit the artifact produced by the Windows Ghostty
   library workflow.
3. Update third-party notices from the real source and build outputs.
4. If a VT header changed or the runtime needs a new symbol, update the allowlist in
   `runtime/crates/ghostty-vt-sys/generate-bindings.sh` and re-run it on each platform (requires
   network access, `libclang`, and `rustfmt`).
5. Update `ghostty_version!` in `runtime/crates/kodosi-pty/src/lib.rs` to match the version in the
   new commit's `build.zig.zon` (reported to programs running inside terminals).
6. Refresh `Ghostty.lock` from the `.version` files, run `./scripts/test.sh`, `just ghostty-check`,
   and `just rust-check`, and test terminals interactively in the apps.
7. Commit the patches, prebuilt artifacts, notices, bindings, and caller changes together.

Preserve all required licenses, corresponding source, and relinking materials for shipped
artifacts.
