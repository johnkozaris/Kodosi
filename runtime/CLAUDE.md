# runtime

Shared terminal runtime, CLI, and native API for macOS and Linux.
Follow `../AGENTS.md` and `../PRODUCT.md` for shared product behavior.

- The host owns each live process and terminal state. New or reconnected views must
  receive current ordered state without disturbing other viewers.
- Input is live and must not be replayed when delivery is uncertain.
- Full control of a shared terminal is intentional. Keep end-to-end encryption and
  sharing behavior aligned with the product's room model.
- Give agents access to shared room context and actions through their existing
  harnesses. Leave provider execution and native data with the provider.
- Keep module ownership direct. Separate crates exist only for real native boundaries.
- Generate desktop and C contracts from current Rust definitions.
- Test both the embedded library and CLI-enabled workspace.
- Keep Ghostty pinned and use isolated storage. Never alter live data or real provider
  histories during tests.
