# Kodosi

Kodosi runs local terminals, connects approved own devices, and gives explicitly
selected friends full terminal control. Missions group people and attached terminals;
they never grant terminal access. Provider integration is limited to native saved
conversation preview/resume and locating native configuration files.

- `rustProcess/` owns the PTY, terminal state, connection lifetime, identity, and
  encrypted transport. `dotnetBackend/` is one ASP.NET application for metadata and
  the bounded relay. Exactly one backend process serves a deployment.
- `../kodosiSwift` and `../KodosiQT` are the native macOS and Linux clients.
  Their build and UI gates remain in their own repositories.
- `Ghostty.lock` must match the Swift lock and pinned `../kodosi-ghostty` package.
  Preserve platform refs, archive digests, and final-link provenance checks.
- `protocol/*.json` defines current cross-stack contracts. Generate the desktop
  manifest and C header from Rust. Do not maintain historical compatibility copies.
- Use the root `justfile` for this repository's gates. Keep tools and dependencies
  project-local. See `docs/DEVELOPMENT.md` for isolated validation.
- Never reset live application databases or erase provider configuration, memory,
  conversations, credentials, or working files during development.
- Closing a view leaves a process running; Stop ends it; quitting its host ends local
  processes. Keep this distinction in runtime, CLI, and client changes.
- Cross-user identity bootstrap is trust on first use. Preserve pinned-device
  verification, revocation, current-publication binding, and bounded queues.
- Keep explanatory comments out of source code; preserve functional tool directives and license notices.
- Keep product scope in `PRODUCT.md`. Encode enforceable rules in types or tests
  rather than expanding architecture documents.
