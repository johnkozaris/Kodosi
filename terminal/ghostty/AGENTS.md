# Ghostty integration

Read `PROVENANCE.md` before changing upstream refs, patches, build tools, notices or
artifacts.

- This package renders host-managed terminals for macOS (AppKit) and iOS (UIKit) and
  provides the Linux and Windows terminal engine. Rust owns processes, terminal state,
  output order and access; never add another process host, parser or fallback
  terminal. Each executable links one Ghostty image.
- `clients/apple/Sources/KodosiTerminal` is the only consumer of `GhosttyTerminal`, and
  `KodosiTerminalSessions` connects it to the runtime. Keep the AppKit and UIKit APIs
  aligned.
- A returning view gets the current state before new output. A stale view never
  controls a replacement terminal. A hidden view keeps its state and takes no input.
  Paste goes straight to the terminal; terminal clipboard requests fail closed.
- Build artifacts only with `./scripts/build.sh`, then run `./scripts/test.sh` and
  `just ghostty-check`. `scripts/verify-ghostty-pins.py` is the only pin check and
  `scripts/digest.py` the only digest code. Never edit artifact metadata to pass a check.
- Keep the Ghostty, Lakr233 and other required licenses and corresponding source.
