# Kodosi

Multiplayer mission control for coding agents on your machine: local-first execution, E2E-encrypted
sharing, and Missions where several humans and agents supervise one session. `room` remains the
internal protocol and backend name for a Mission. Execution and content keys never leave the owner's
device; the backend stores product metadata and relays ciphertext. Cross-user identity bootstrap is
TOFU — active-backend substitution before the first pin stays outside the current threat model.

- `rustProcess/` runtime, `dotnetBackend/` durable state and relay. `protocol/*.json` contains
  normative parity manifests; the desktop-runtime manifest is generated from Rust types.
- The root `justfile` owns this repository's gates. The shipping Swift client's gates live in
  `../kodosiSwift/Justfile`.
- `../kodosiSwift` is a sibling checkout and the one shipping client; Electron is discontinued.
- `Ghostty.lock` mirrors `../kodosiSwift/Ghostty.lock` and pins the committed
  package/upstream/archive relationship; the parity gate rejects drift.
- Direction: `docs/review-2026-06/PRODUCT-DIRECTION.md`.
- Encode a rule as a type, test, or manifest before writing prose about it.
