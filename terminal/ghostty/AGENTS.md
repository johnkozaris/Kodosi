# Kodosi Ghostty agent guide

Read `PROVENANCE.md` before changing upstream refs, patches, build tools,
notices, or native artifacts.

This package supplies Kodosi's macOS AppKit and iOS UIKit Metal terminal views
and Linux terminal engine. Rust owns processes, terminal state, output order,
and access. The host-managed adapter displays that state; it must not become
another process host, parser, or fallback terminal.

`../../clients/apple/Sources/KodosiTerminal` owns the SwiftUI integration and is
the only consumer that imports `GhosttyTerminal`. Read
`../../clients/macos/docs/GHOSTTY_INTEGRATION.md` before changing public APIs.
Keep the AppKit and UIKit views' host-managed APIs aligned. Preserve native
input, selection, accessibility, checkpoint, and clipboard behavior.

Build native artifacts only through `./Script/build.sh`, then run the package
tests and relevant artifact, notice, and sanitizer checks in `Script/`.
Never edit artifact metadata to hide a failed check or publish uncommitted pins.
Keep the Ghostty, Lakr, and other required licenses and corresponding source.

Keep source free of explanatory comments; retain directives and licenses.
Test changed behavior at the native boundary. Validate user workflows manually,
not with automated smoke drivers.
