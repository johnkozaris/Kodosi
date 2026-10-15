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

1. Review the upstream changes and rebase `Patches/ghostty`. A patch that upstream
   already contains fails the build: remove it.
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

## Patches

Audit of 2026-10-15 against Ghostty main `ad97d331` (2026-10-14) and its 122 open pull
requests; `#` numbers in this section are pull requests of `ghostty-org/ghostty`. All 23
patches still apply as text to that commit. Main has the behavior of none of them, and no
open pull request supplies it. Lines are those of the patch file, then the source lines
that it adds and removes.

Builds: **Apple** is the combined image in `Vendor/GhosttyKit.xcframework` (macOS, iOS,
iOS Simulator); the macOS runtime links the same image. **VT** is libghostty-vt in every
artifact: Linux, Windows and the VT part of the Apple image.

| Patch | Lines (+/−) | Builds | Reason | Ghostty main and open pull requests | Result |
|---|---|---|---|---|---|
| 0001 darwin-libghostty-install | 32 (+10/−11) | Apple | Installs the static `libghostty.a` on Darwin, so that Kodosi packages its own XCFramework. | Main installs nothing on Darwin. | Keep |
| 0002 host-managed-io | 627 (+369/−37) | Apple | A termio backend without a PTY: the runtime writes output with `ghostty_surface_write_buffer` and gets input and resizes through callbacks. iOS cannot start processes. | Not on main. #14715 changes the same `Surface.init` code. | Keep |
| 0006 embedded-renderer-trim | 195 (+63/−38) | Apple | From libghostty-spm. The `custom-shaders` option removes glslang and SPIRV-Cross. It needs the `inspector` field of 0007. | No such option. | Keep; merge with 0007 |
| 0007 disable-inspector | 260 (+105/−6) | Apple | From libghostty-spm. The `inspector` option replaces the inspector with stubs and removes Dear ImGui. | No such option. | Keep |
| 0010 fix-scroll-remainder-zeroing | 22 (+2/−2) | Apple | Precise scrolling loses part of a row: `poff / cell_size` keeps its fraction, so the saved remainder is always zero. | The fault is still on main. | Send upstream |
| 0011 local-metal-toolchain | 30 (+10/−2) | Apple | `GHOSTTY_METAL` and `GHOSTTY_METALLIB` select the pinned Metal toolchain. | Main calls `xcrun` only. | Keep |
| 0012 normalize-public-header-whitespace | 41 (+5/−5) | Apple, VT | Removes trailing spaces from 6 header lines. Other headers keep theirs, and `Vendor/**` is `-whitespace`. | Main has the spaces. | Remove at the next rebuild |
| 0013 formatter-portable-replay | 345 (+152/−46) | VT | Formats a chosen screen, and writes OSC 8 links in VT output, for the VT replay that 0017 replaced. Only the CLI mirror (`format_finite_cli_replay`) uses it: it asks for the active screen, which is the default, and gets the links. | Not on main. | Remove (the CLI mirror then shows links as plain text), or send the OSC 8 part upstream |
| 0014 formatter-explicit-screen-modes | 92 (+59/−8) | VT | With a chosen screen, omits the alternate-screen modes and writes CSI H. The CLI mirror asks for no modes and writes its own CSI H, so this has no effect. | Not on main. #14557 and #14200 change the same mode code. | Remove |
| 0015 combined-renderer-vt-library | 81 (+14/−2) | Apple | `emit-combined-lib`: one image exports the renderer API and the VT API, so the app and the runtime share one terminal implementation. | Not on main. | Keep |
| 0016 host-managed-terminal-policy | 251 (+155/−2) | Apple | (a) `ghostty_surface_replace_buffer`, a VT replay that 0017 removes again. (b) `ghostty_clipboard_request_type`, so that the app allows only user paste; the core already denies program reads with `clipboard-read = deny`. (c) Kitty and Glyph protocols off when `image-storage-limit = 0`. | Not on main. Main turns Kitty off at 0 but not the Glyph protocol, although its documentation says "all image protocols". | Remove (a), keep (b), send (c) upstream |
| 0017 semantic-json-checkpoint | 6345 (+5788/−113) | VT, Apple | The Kodosi JSON checkpoint: `ghostty_checkpoint_*` in libghostty-vt (4 new files, about 3,900 code and 1,170 test lines) and `ghostty_surface_checkpoint_restore` in the app (about 400 lines). | Main and the pin have the binary snapshot `ghostty_snapshot_*`, which keeps the same state. See below. | Replace the VT part with the snapshot |
| 0018 host-managed-report-provenance | 334 (+89/−19) | Apple | Tags the replies that the terminal itself makes (every reply of the stream handler, and size, focus, color scheme, visibility, clipboard and title reports) and drops them in the host-managed backend, so that only the runtime's terminal answers a program. | Not on main. | Keep |
| 0019 disable-combined-tmux-viewer | 107 (+28/−8) | Apple | Turns off the tmux control-mode viewer in the combined image. The runtime's terminal does not have it, so the view and the host would differ. | Main ties it to Oniguruma. | Keep |
| 0020 disable-host-managed-paste-events | 13 (+2/−1) | Apple | A host-managed surface ignores the Kitty paste events mode: the program gets text, never a clipboard listing to read. | Not on main. | Keep |
| 0020 linux-vt-export-surface | 235 (+218/−0) | Linux `.so` | A version script, so that the shared library exports only the 208 `ghostty_*` functions. No product links the `.so`: the runtime and the Qt app link `libghostty-vt.a`. | No version script on main. | Remove with the unused `.so` |
| 0021 checkpoint-restore-mailbox-safety | 717 (+423/−104) | Apple | Fixes the surface restore of 0017: one coalesced notification instead of a thread for each restore, a renderer handshake before the old terminal is freed, and retries when a mailbox is full. | Not on main. #14444 changes the same `App.drainMailbox`. | Fold into the surface restore |
| 0022 surface-configuration-ownership | 115 (+20/−5) | Apple, VT | A config for each surface (font size, scrollback limit), scrollback changes for open surfaces, and two ownership fixes to the restore of 0017. | Not on main. Main documents that scrollback changes reach only new surfaces. | Keep; fold the fixes into the restore |
| 0023 ignore-zero-width-format-characters | 40 (+15/−4) | Apple | Font selection ignores ZWSP, ZWNJ, WJ and BOM in a grapheme, as it ignores VS15, VS16 and ZWJ. Otherwise a prompt's ZWSP after a symbol rejects every symbol font. | The fault is still on main. | Send upstream |
| 0024 checkpoint-history-row-limit | 174 (+32/−16) | VT | `max_history_rows`, for the visible checkpoint (0 rows) that a remote viewer gets before the full one. | The snapshot encoder has no history limit. | Replace (see below) |
| 0025 checkpoint-restore-surface-colors | 20 (+10/−0) | Apple | A restore keeps the surface's configured colors, so that terminals stay dark. | Not on main. | Fold into the surface restore |
| 0026 ios-combined-image | 45 (+7/−1) | Apple (iOS) | Builds the combined image for iOS: Metal SDK and version flags, and `-fblocks`. | Upstream removed iOS from the full build in `7a171895` (2026-08-12). #11010 (visionOS, open since February) would add an embedded Apple platform. | Keep; cannot go upstream; a risk for the iPhone app |
| 0027 libintl-only-with-i18n | 23 (+3/−2) | Apple (iOS) | Links libintl only with i18n; the iOS slices build without i18n. | Main always links it on Apple. | Send upstream |

The checkpoint patches (0013, 0014, 0017, 0021, 0024, 0025) are 7,693 of the 10,144
lines: +6,464/−287 source lines. About 5,600 of the added lines are in libghostty-vt and
about 830 in the Apple surface.

### Checkpoint and the upstream snapshot

The public C API lets the runtime read the size, the cursor (position, style, visibility,
pending wrap), the active screen, every mode, the colors and palette, the current Kitty
keyboard flags, the mouse shape and tracking, the title, the pwd, the scrollback limits
and row counts, the unfinished input bytes, and each row and cell of both screens (text,
graphemes, style, hyperlink URI, wrap and prompt marks) through grid references. It cannot
read the saved cursor (DECSC), the pen and charsets of the cursor, tab stops, scrolling
margins, the Kitty keyboard stack, saved modes, the previous character for REP, explicit
OSC 8 ids or page capacities. Apart from modes, colors, title, pwd, scrollback limits and
size, state goes back into a terminal only as VT bytes. So a checkpoint made by the runtime
would be a replay that loses this state; 0017 replaced the replay of 0016 for that reason.

The upstream snapshot (`include/ghostty/vt/snapshot.h`, unchanged from the pin to main)
keeps this state, but `runtime/crates/ghostty-vt-sys/generate-bindings.sh` blocks it. A
probe linked `Vendor/GhosttyVt/macos-arm64`, encoded terminals with both codecs, decoded
each snapshot, and encoded the copy as JSON again:

| Terminal | JSON, all history | Snapshot, all history | JSON, visible | Snapshot to READY |
|---|---|---|---|---|
| 120×40, 1,024 history rows | 2,456,739 B; encode 8.8 ms, restore 22.0 ms | 471,618 B; 0.2 ms, 0.5 ms | 104,868 B | 104,430 B |
| 120×40, alternate screen | 2,554,175 B | 491,160 B | 202,304 B | 123,956 B |
| 400×100, 1,024 history rows | 8,481,740 B, more than the runtime's 8 MiB limit | 453,257 B | 780,778 B | 59,433 B |
| 200×60, 10,000 history rows | 39,198,158 B; 179.6 ms, 359.1 ms | 4,610,888 B; 3.1 ms, 8.2 ms | 244,168 B | 114,922 B |

- The JSON of each copy is equal to the JSON of its source, except page capacities and the
  scrollback limit of the alternate screen (`null` and `0`, which both mean none). This
  includes a pending wrap with a saved cursor, SS2 and SS3, and input that stops inside a
  CSI, a UTF-8 sequence or an OSC: after the same next input, the source and the copy
  still format to the same bytes, although their pages can split at other rows.
- A snapshot cut after its READY record decodes with `ghostty_snapshot_decoder_ready` to
  the same visible state as the JSON visible checkpoint.
- The snapshot does not keep page capacities, and scrollback is pruned page by page.
  After 5,000 more lines, the copy kept 1,000 history rows and the source 833; the rows
  that the source kept were the same in the copy.

What is missing upstream:

1. A history limit in the encoder (0024). The runtime can cut after READY, because the
   record frame is documented, or upstream can add an encode option.
2. A format promise. Version 1 still changes: #14557 and #14200 change what it writes
   in the TERMINAL record. Peers must use the same Ghostty commit, so the checkpoint
   version in the protocol must follow `Ghostty.lock`.
3. A restore into an existing terminal or surface. The runtime, the CLI and the Qt app
   already restore into a new terminal. The Apple surface needs a smaller Kodosi patch
   that decodes with the upstream Zig codec and keeps the terminal swap of 0017, 0021,
   0022 and 0025.

Estimate: 2 to 3 days for the runtime, the CLI, the Qt app and the bindings; 3 to 4 days
for the new surface restore, its tests and checks in the Mac and iPhone apps; 1 to 2 days
to rebuild and verify the Apple, Linux and Windows artifacts. That is 6 to 9 working days,
with no pin move. The patch set goes from about 10,100 to about 3,300 lines.

### Other findings

- The Linux library comes from the patch set of `addf8fc0` (`patches_sha256=3d8bc75e…`,
  Kodosi issue #74). It differs from the current set only by 0026, 0027 and README.md,
  which change Apple builds only.
- The series keeps its history: 0016 adds a replay that 0017 removes, and 0021, 0022
  and 0025 fix 0017. One patch for each requirement would be smaller.
- These upstream pull requests change code that the patches change, so they will conflict
  at the next rebase: #14715 (0002), #14444 (0021), #14557 and #14200 (0014, and
  `stream_handler.zig` of 0016, 0018, 0019 and 0021), and #14519, which moves Ghostty to
  Zig 0.17 (Kodosi pins 0.16.0).
