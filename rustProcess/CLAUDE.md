# rustProcess

Arm64 macOS embedded runtime and CLI, and the source of truth for live sessions: terminal,
supervision, identity, and sharing. Windows runtime/CLI support is retired; do not add a
platform branch without restoring the complete PTY, storage/security, Ghostty artifact, and CI
support boundary. Rust applies every ordered PTY batch to the sole headless libghostty-vt
authority before publishing the same raw bytes to native renderers; there is no test-only runtime
path. C ABI obligations the generated header cannot express live in
`crates/kodosi-ffi-c` tests and the Swift embedder's protocol/ABI window.
