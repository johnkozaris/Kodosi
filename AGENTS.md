# Kodosi

Read `PRODUCT.md` first. Before changing a client's interface, read that client's
`PRODUCT.md` and `DESIGN.md`.

## Projects

- `runtime/`: the Rust runtime, `kodosi` CLI and C API. It owns terminals, identity,
  encryption and rooms; the host of a terminal owns its process and state. The iOS app
  uses the viewer build (`--no-default-features`), which hosts nothing.
- `backend/`: ASP.NET Core service that stores metadata and relays encrypted traffic.
  It reads only each terminal's agent and state (`TerminalStatus`), never its text. Run
  one serving process: it holds live sessions behind a PostgreSQL lease.
- `clients/macos/`, `clients/ios/`: Swift apps that share `clients/apple/`. On macOS the
  signed app executable is also the `kodosi` CLI, so both use the app's Keychain
  identity. The iOS app views terminals and starts none.
- `clients/qt/`: Qt app for Linux and Windows. QML only presents the runtime's state.
- `sign-in/`: Keycloak theme for the sign-in pages, account page and e-mails. Keep
  Keycloak's form field names and ids.
- `protocol/`: contracts. `just protocol-gen` writes `desktop-runtime.json`, the
  `kodosi-ffi-c` build writes the C header, and the rest are written by hand.
- `VERSION`: the one version of all apps and the CLI.
- `terminal/ghostty/`: pinned, patched Ghostty, selected by root `Ghostty.lock`. Read
  its `PROVENANCE.md` first; every file in `Patches/ghostty/` is part of the pinned
  digest.

## Rules

- Keep full terminal control. Don't add permission tiers, sandboxing or agent workflow
  machinery.
- Sharing is trust. People who share a terminal or a room, and the programs and agents
  in it, are trusted: don't restrict what they can do (clipboard, links, escape
  sequences, commands). Prompt injection is the agent provider's concern. Protect only
  against the network, the backend and people without access.
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
