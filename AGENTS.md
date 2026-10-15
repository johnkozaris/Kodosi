# Kodosi

Read `PRODUCT.md` first, and the `AGENTS.md` nearest to the files you change.

- `runtime/`: Rust runtime, CLI and C API. Owns terminals, identity, encryption and rooms.
- `backend/`: ASP.NET Core service. Stores metadata, relays encrypted traffic. Run one
  serving process.
- `clients/macos/`, `clients/ios/`: Swift apps. `clients/apple/` is their shared package.
  The iOS app views terminals and hosts none.
- `clients/qt/`: Qt app for Linux and Windows.
- `sign-in/`: Keycloak theme for the sign-in pages, account page and e-mails.
- `protocol/`: contracts generated from Rust.
- `terminal/ghostty/`: pinned Ghostty integration; root `Ghostty.lock` selects it.

Rules:

- Keep full terminal control. Don't add permission tiers, sandboxing or agent workflow
  machinery.
- Leave provider execution, permissions, settings and conversations with the provider.
- Preserve end-to-end encryption, chosen sharing, identity continuity, bounded resource
  use and ordered terminal state.
- Kodosi has no release: keep no code for earlier protocol versions or saved formats.
- Never reset live databases or touch real provider data, credentials, conversations,
  memory or working files.
- Keep dependencies project-local. Run checks through the root `justfile`; name files,
  recipes and scripts by `docs/DEVELOPMENT.md`.
- Write clear code and focused tests, not comments or architecture prose. Keep required
  directives and license notices.
- Validate real workflows with native or boundary tools. Don't add automated smoke
  drivers.
