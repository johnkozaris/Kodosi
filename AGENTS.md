# Kodosi

Read `PRODUCT.md` for product intent and current behavior. Kodosi
brings people and agents together through room-shared terminals and conversation.
Full terminal control is intentional. Do not turn missing capabilities into product
exclusions or add permission tiers, sandboxing, or agent workflow machinery by default.

- `runtime/` owns live terminals, identity, and encrypted endpoint connections.
- `backend/` stores shared metadata and routes encrypted terminal traffic. Run one serving
  backend process.
- `clients/macos/` and `clients/linux/` are the native macOS and Linux clients.
- `protocol/` contains the current cross-stack contracts generated from Rust. Keep the
  native Ghostty pins and provenance checks aligned with `terminal/ghostty/`.

- Minimize dismisses a terminal view without ending its process; Close ends the
  terminal; quitting its host ends local processes.
- Give agents room context and actions through their existing harnesses. Leave provider
  execution, permissions, settings, and native conversation storage with the provider.
- Preserve end-to-end encryption, chosen sharing, identity continuity, bounded resource
  use, and ordered terminal state.
- Never reset live databases or alter real provider data, credentials, conversations,
  memory, or working files during development.
- Keep dependencies project-local and use the root `justfile` for repository gates.
- Keep current product intent in `PRODUCT.md` and setup in `docs/DEVELOPMENT.md`.
- Prefer clear code and focused tests over explanatory source comments or architecture
  prose. Preserve required directives and license notices.

Validate real workflows with the appropriate native or boundary tools. Do not add
automated smoke drivers.
