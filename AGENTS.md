# Kodosi

Kodosi is a native app where people and their coding agents work together in shared terminals and
projects. Read [PRODUCT.md](PRODUCT.md) first for the product intent and core decisions. Before
changing any client's UI, also read that client's `PRODUCT.md` and `DESIGN.md`.

## Repository layout

- `runtime/`: the Rust runtime, `kodosi` CLI, and C FFI. Each computer runs one host process that
  owns its terminals, identity, encryption, and projects. Mobile apps use the viewer build, which
  hosts no terminals.
- `backend/`: the ASP.NET Core server. It stores metadata and relays encrypted traffic, reading
  only each terminal's agent type and state, never terminal text.
- `clients/macos/` and `clients/ios/` (Swift, with shared code in `clients/apple/`),
  `clients/qt/` (Linux and Windows), and `clients/android/`. Clients render runtime state; logic
  that must behave identically across clients belongs in `runtime/`.
- `documents/` (live documents) and `browser/` (shared browser): standalone subsystems used as
  libraries by Kodosi. Neither imports anything from Kodosi.
- `sign-in/`: the Keycloak theme for sign-in pages, the account page, and emails.
- `protocol/`: shared contracts between the runtime, clients, and backend.
- `terminal/ghostty/`: pinned, patched Ghostty integration. Read its `PROVENANCE.md` before making
  changes.

## Product rules

- Keep full terminal control. Don't add permission tiers, sandboxing, or agent workflow machinery.
  Programs in a terminal get standard Ghostty behavior.
- Sharing means trust. Don't restrict what participants or their agents can do in a shared
  terminal or project. Protect against the network, the backend, and unauthorized users.
- Leave provider execution, permissions, settings, and conversation storage with the provider.
- Preserve end-to-end encryption, explicit sharing, identity continuity, bounded resource use, and
  ordered terminal state.

## Working rules

- Kodosi has a small user base, and users update all their devices with each release. Make clean
  changes without carrying compatibility shims for older builds, saved data, or protocol versions.
  Changes to the live database still require a new EF Core migration.
- Never reset live databases or touch real provider data, credentials, conversations, memory, or
  working files.
- Keep dependencies project-local. Run checks through the root `justfile`.
- Verify changes locally before opening a pull request
  ([Before a pull request](docs/DEVELOPMENT.md#before-a-pull-request)). Don't rely on CI to catch
  what your machine can check.
- Translate every new user-facing string into all supported languages
  ([Texts and languages](docs/DEVELOPMENT.md#texts-and-languages)).
- Validate real workflows with native or boundary tools. Don't add automated smoke drivers.

## Code and tests

- Let names carry the meaning. Only write comments for context the code can't express, such as a
  platform bug or a security constraint. Never restate what the code does. Keep required
  directives and license notices.
- Keep a test only if it can catch a real regression in behavior that users or other components
  depend on. Don't test constants, defaults, generated code, mocks, language/framework behavior,
  or things the test itself set up.
- Run each check where it catches bugs earliest: on the developer's machine. Pull requests only
  repeat the fast checks, while the nightly run on `main` builds the Linux/Windows apps and
  Windows package. Don't add redundant checks for things a build, test, or existing check already
  catches.
- One job, one implementation. When a change makes code unused, delete it along with its settings,
  strings, assets, scripts, and tests.

## Documentation

Docs should explain intent, design decisions, and how the parts fit together. Leave implementation
details in the code: don't duplicate limits, timeouts, sizes, file paths, key bindings, screen
layouts, or step-by-step function walkthroughs. Don't commit plans, scratch notes, status updates,
release notes, or changelogs; those belong in issues, pull requests, and commits.

Each doc has a single purpose. State each fact once, describing the current state, in the file that
owns it:

- `README.md`, `docs/GETTING_STARTED.md`, and `docs/SECURITY.md`: user-facing documentation. Keep
  their translations in `docs/<language>/` updated in the same change.
- `PRODUCT.md`: product scope, audience, and cross-cutting decisions.
- Client `PRODUCT.md` and `DESIGN.md`: behavior and visual design specific to that client.
- `docs/ARCHITECTURE.md` and `docs/CRYPTOGRAPHY.md`: system architecture and encryption design.
- `docs/DEVELOPMENT.md`, `docs/SELF-HOSTING.md`, and subproject `README.md` files: building,
  running, checking, and self-hosting.
