# Provenance and updates

The integration began from `Lakr233/libghostty-spm` tag `1.3.1` (commit
`b0930320739324886590e865d571eb5dd7073912`); the UIKit view adapts that commit's iOS
code. Keep its MIT attribution and Ghostty's license. This is not an official Ghostty
distribution.

## Pins

`MacOSGhostty.ref`, `LinuxGhostty.ref` and `WindowsGhostty.ref` select the upstream
commit of each platform; iOS uses the macOS ref. Apple platforms get one image per slice
in `Vendor/GhosttyKit.xcframework`; Linux and each Windows architecture get a VT library
in `Vendor/GhosttyVt`. Each artifact's `.version` file records its measured inputs and
outputs. `VERSION` names this package, not the Ghostty version.

Root `Ghostty.lock` has `<platform>_ghostty_commit` (the commit of the platform's ref)
and `<artifact>_archive_sha256` (the archive digest of an Apple slice or VT layout, with
underscores for hyphens, as in `ios_arm64_simulator_archive_sha256`).

`NativeBuild.env` is the only record of the Apple slices, Zig targets and i18n options;
scripts read it rather than copying values. The iOS version in
`GHOSTTY_BUILD_IOS_TARGET` must not exceed `clients/apple/Config/iOSDeploymentTarget`
and must match `Package.swift`; `just mac-check` and `just ios-check` check this.

## Checks

`scripts/verify-ghostty-pins.py` is the only pin check: it compares the lock, refs,
build records and `.version` files with the vendored files. `--working-tree` skips the
lock (never publish such a build), `--vendor <folder>` checks another folder,
`--platform <macos|linux|windows>` checks one platform, and `--patched-source <folder>`
also checks the patched source of a build. `scripts/digest.py` is the only digest code.
`just ghostty-check` runs the verifier, the digest tests and the host's notice checks.

The other `verify-*` scripts probe the native artifacts and compare the notices with
them; `ghostty-vt-sys` checks its target's artifact against the lock because Cargo
cannot run Python. On Windows, Git writes the Linux artifact's symlinks as files, so the
verifier skips that folder digest there.

## Update Ghostty

1. Review the upstream changes and rebase `Patches/ghostty`; drop patches that upstream
   makes unnecessary.
2. Rebuild each platform with `./scripts/build.sh` on that platform; it applies the
   patches to a clean checkout, builds, verifies and records digests.
3. Update the notices from the real source and artifacts.
4. If a VT header changed, or the runtime needs a new Ghostty symbol (add it to the
   allowlist first), run `runtime/crates/ghostty-vt-sys/generate-bindings.sh` on that
   platform (or pass `macos-arm64`, `linux-x86_64`, `windows-x86_64` or
   `windows-arm64`). It needs network access, libclang and rustfmt.
5. Run `./scripts/test.sh`, update `Ghostty.lock` from the `.version` files, and run
   `just ghostty-check` and `just rust-check`.
6. Test the Rust, Swift and Qt integration and the terminal lifecycle.
7. Commit the patches, artifacts, notices, bindings and consumers together.

Keep the licenses, corresponding source and relinking materials that the artifacts
require. Builds need not be byte-for-byte reproducible.
