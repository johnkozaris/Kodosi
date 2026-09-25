# Kodosi

Kodosi runs terminals on the host computer. Approved own devices can use them, and
friends can use only the terminals explicitly shared with them. Everyone admitted to
a terminal has full control. Missions organize people and terminals but never grant
terminal access.

- `runtime/` owns live terminals, identity, and encrypted endpoint connections.
- `backend/` stores shared metadata and routes encrypted terminal traffic. Run one serving
  backend process.
- `../kodosiSwift` and `../KodosiQT` are the native macOS and Linux clients.
- `protocol/` contains the current cross-stack contracts generated from Rust. Keep the
  native Ghostty pins and provenance checks aligned across repositories.

- Minimize dismisses a terminal view without ending its process; Close ends the
  terminal; quitting its host ends local processes.
- Provider support is limited to previewing or resuming native saved conversations and
  locating native configuration. Do not wrap providers or take over their permissions.
- Preserve identity continuity, explicit sharing, revocation, bounded resource use, and
  ordered terminal state.
- Never reset live databases or alter real provider data, credentials, conversations,
  memory, or working files during development.
- Keep dependencies project-local and use the root `justfile` for repository gates.
- Keep current product intent in `PRODUCT.md` and setup in `docs/DEVELOPMENT.md`.
- Prefer clear code and focused tests over explanatory source comments or architecture
  prose. Preserve required directives and license notices.

Validate real workflows with the appropriate native or boundary tools. Do not add
automated smoke drivers.
