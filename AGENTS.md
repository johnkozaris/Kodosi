# Kodosi

Kodosi is a native app where people and their coding agents work together in shared
terminals and projects. Read [PRODUCT.md](PRODUCT.md) first: it has the intent and the
decisions. Before you change a client's interface, read that client's `PRODUCT.md` and
`DESIGN.md`.

## Layout

- `runtime/`: the Rust runtime, the `kodosi` command and the C API. Each computer runs
  it as one host, which owns that computer's terminals, identity, encryption and
  projects. The phone apps use its viewer build, which hosts nothing.
- `backend/`: the ASP.NET Core service. It stores metadata and relays encrypted
  traffic, and reads only each terminal's agent and state, never its text.
- `clients/macos/` and `clients/ios/` (Swift, with shared code in `clients/apple/`),
  `clients/qt/` (Linux and Windows) and `clients/android/`. A client presents the
  runtime's state; what all clients must decide the same way belongs in the runtime.
- `documents/` (live documents) and `browser/` (the shared browser): separate systems
  that Kodosi uses as libraries. They import nothing of Kodosi.
- `sign-in/`: the Keycloak theme of the sign-in pages, the account page and the e-mails.
- `protocol/`: the contracts between the runtime, the apps and the backend.
- `terminal/ghostty/`: pinned, patched Ghostty. Read its `PROVENANCE.md` before a change.

## Product rules

- Keep full terminal control. Don't add permission tiers, sandboxing or agent workflow
  machinery. Programs in a terminal get what Ghostty gives them.
- Sharing is trust. Don't restrict what the people in a shared terminal or project, or
  their agents, can do there. Protect against the network, the backend and people
  without access.
- Leave provider execution, permissions, settings and conversations with the provider.
- Preserve end-to-end encryption, chosen sharing, identity continuity, bounded resource
  use and ordered terminal state.

## Working rules

- Kodosi is released but has no users yet. Until it has users, make clean changes: don't
  keep support for earlier builds, their saved data or their protocol. Each change of
  the live database is still a new migration.
- Never reset live databases or touch real provider data, credentials, conversations,
  memory or working files.
- Keep dependencies project-local. Run checks through the root `justfile`.
- Give each new text that people read its words in each language
  ([Texts and languages](docs/DEVELOPMENT.md#texts-and-languages)).
- Validate real workflows with native or boundary tools. Don't add automated smoke
  drivers.

## Code and tests

- Names carry the meaning. Write a comment only for a reason that the code cannot show,
  such as a fault of a platform or a security limit. Never describe what the code does.
  Keep required directives and license notices.
- A test stays only if it can fail for a real fault in behavior that people or another
  part rely on. Don't test constants, defaults, generated code, a stand-in, the language
  or the framework, or what the test itself set up.
- A check runs in one place, where it finds its fault first. A pull request compiles and
  tests each platform. Package, signature and release checks run when their own files
  change. Don't add a check for what a build, a test or another check already finds.
- One job has one implementation. In the change that makes something unused, remove it
  with its settings, texts, assets, scripts and tests.

## Documents

A document gives intent, position and direction: what Kodosi is, why it works as it does
and how the parts fit. The code holds the rest. Don't write what the code shows, such as
limits, timings, sizes, file names, keys, the layout of a screen or the steps of a
function. Don't add plans, notes, status, release notes or history; they belong in
issues, pull requests and commits.

Each document has one job. Say a thing once, as it is now, in the document whose job it
is.

- `README.md`, `docs/GETTING_STARTED.md` and `docs/SECURITY.md`: for the people who use
  Kodosi. `docs/<language>/` has their translations; change them in the same change.
- `PRODUCT.md`: what Kodosi is, who it is for and the decisions that every part keeps.
- A client's `PRODUCT.md`: only what is particular to that app. Its `DESIGN.md`: its
  visual system.
- `docs/ARCHITECTURE.md`: how the parts work together. `docs/CRYPTOGRAPHY.md`: the
  encryption design.
- `docs/DEVELOPMENT.md` and each project's `README.md`: build, run and check.
  `docs/SELF-HOSTING.md`: run a backend.
