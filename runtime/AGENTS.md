# Runtime

The Rust runtime, `kodosi` CLI and C API for macOS, Linux and Windows. iOS uses the
viewer build (`--no-default-features`), which opens terminals hosted elsewhere and
hosts none.

- The host owns each process and its terminal state. New and reconnected views get the
  current ordered state without disturbing other viewers. Never replay input whose
  delivery is uncertain.
- Keep module ownership direct; separate crates exist only for native boundaries.
- `just protocol-gen` writes `protocol/desktop-runtime.json`. The `kodosi-ffi-c` build
  writes the C header; keep `clients/apple/Sources/KodosiKit/kodosi_runtime.h` equal to it.
- Test the embedded library, the CLI workspace and the viewer build: `just rust-check`
  and `just ios-rust-lint`.
- Tests use isolated storage and never touch live data or real provider histories.
