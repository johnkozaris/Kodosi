# rustProcess

Shared runtime, CLI, and C ABI for arm64 macOS and x86_64 Linux. Each live session owns
its PTY, process, Ghostty terminal state, ordered output, and current audience.

- Apply each PTY output batch to the host's Ghostty authority before publishing it.
  New views receive a fresh checkpoint and ordered continuation without resetting
  other viewers.
- Input is live transport, not a durable job. Reject stale session/connection
  identities and never replay uncertain input after reconnect.
- Approved own devices and explicitly selected friends have full terminal control.
  Device identity and sharing administration remain owner operations.
- Provider history/configuration is on demand and read-only. Shell launch must not
  install hooks or wrappers, intercept permissions, or inject provider telemetry.
- Runtime modules own their domain types. Separate crates are limited to the native
  PTY, Ghostty bindings/wrapper, and C ABI boundaries.
- The default `cli` feature adds the command-line client. Test the embedded library
  with default features disabled as well as the complete workspace.
- Generate the desktop manifest and C header from current Rust definitions. Keep
  callback lifetime and terminal bootstrap tests in `crates/kodosi-ffi-c`.
- Keep Ghostty pinned and use isolated storage for tests. Do not reset live data or
  mutate provider histories to make a test pass.
