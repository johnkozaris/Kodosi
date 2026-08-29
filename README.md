# Kodosi

**Multiplayer mission control for coding agents on your machine.**

Kodosi runs Claude Code and Copilot CLI in local shells and lets a developer supervise
them from the same Mac or another enrolled Mac. It adds fail-closed control and makes a
live session shareable through E2E-encrypted Missions: pull a teammate in to watch or
steer, let authorized participants approve or deny pending requests, and hand out a
time-boxed guest pass. The CLI, protocol, and backend call a Mission a `room`. Execution
and content keys never leave the owner's device; the backend stores product metadata and
relays ciphertext.

Cross-user identity bootstrap is trust-on-first-use. Device-list and roster changes,
signatures, and rollbacks fail closed after the first pin, but an actively malicious
backend at first contact stays outside the threat model until safety-number verification
or independent key transparency ships.

`rustProcess/` is the arm64 macOS runtime, CLI, and C ABI; Windows runtime/CLI support is
retired, and Linux is used only by the backend services. `dotnetBackend/` is the REST,
PostgreSQL, and WebSocket relay. `protocol/` contains normative cross-stack manifests, while the
desktop runtime manifest is generated from Rust wire types. The shipping SwiftUI macOS client lives
in the sibling `../kodosiSwift` checkout and links the runtime directly. The sibling
`../kodosi-ghostty` package provides the pinned native renderer and headless terminal
engine.

- Product definition: [`PRODUCT.md`](PRODUCT.md)
- Product direction: [`docs/review-2026-06/PRODUCT-DIRECTION.md`](docs/review-2026-06/PRODUCT-DIRECTION.md)
- Development guidance: [`CLAUDE.md`](CLAUDE.md); this repository's gates run from the root
  `justfile`, while the shipping client's gates run from `../kodosiSwift/Justfile`
