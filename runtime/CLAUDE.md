# runtime

Shared terminal runtime, CLI, and native API for macOS and Linux.

- The host owns each live process and terminal state. New or reconnected views must
  receive current ordered state without disturbing other viewers.
- Input is live and must not be replayed when delivery is uncertain.
- Approved own devices and explicitly selected friends have full terminal control;
  identity and sharing changes remain owner actions.
- Provider history/configuration is on demand and read-only. Shell launch must not
  wrap providers or take over their permissions.
- Keep module ownership direct. Separate crates exist only for real native boundaries.
- Generate desktop and C contracts from current Rust definitions.
- Test both the embedded library and CLI-enabled workspace.
- Keep Ghostty pinned and use isolated storage. Never alter live data or real provider
  histories during tests.
