# Kodosi

Multiplayer mission control for coding agents on your machine: local-first execution, E2E-encrypted
sharing, and Missions where several humans and agents supervise one session. `room` remains the
internal protocol and backend name for a Mission. Execution stays on the owner's device; content keys
are wrapped only to authorized endpoint devices. The backend stores metadata and relays ciphertext. Cross-user identity bootstrap is
TOFU — active-backend substitution before the first pin stays outside the current threat model.

- `rustProcess/` runtime, `dotnetBackend/` durable state and relay. `protocol/*.json` contains
  normative parity manifests; the desktop-runtime manifest is generated from Rust types.
- The root `justfile` owns this repository's gates. The shipping Swift client's gates live in
  `../kodosiSwift/Justfile`.
- `../kodosiSwift` is the shipping macOS client; `../KodosiQT` is the Linux client under
  development against the same runtime. Electron and Windows runtime/client support are discontinued.
- `Ghostty.lock` mirrors `../kodosiSwift/Ghostty.lock` and pins the committed
  package plus separate macOS and Linux upstream/archive relationships. The
  parity gate checks `MacOSGhostty.ref` and `LinuxGhostty.ref` directly and
  rejects drift.
- Direction: `docs/PRODUCT-DIRECTION.md`.
- Encode a rule as a type, test, or manifest before writing prose about it.
