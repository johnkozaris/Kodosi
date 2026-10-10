# Kodosi documentation

Kodosi is a native workspace for people, coding agents, and shared terminals.
Start with the guide for the job you want to do.

## Use Kodosi

- [Getting started](GETTING_STARTED.md): what you need, then sign in, open a terminal,
  start an agent, and work in a room. With pictures.
- [Agents and tasks](AGENTS_AND_TASKS.md): room chat, the bundled skill, shared tasks,
  and GitHub or Gitea issues.
- [Questions and answers](FAQ.md): short answers about the app, agents, sharing, and
  the service.
- [Security](SECURITY.md): what is encrypted, what sharing grants, and private reporting.

## Build and operate

- [Development](DEVELOPMENT.md): repository layout, toolchains, builds, and checks.
- [macOS build guide](../clients/macos/README.md) and
  [Linux build guide](../clients/linux/README.md).
- [Self-hosting](SELF-HOSTING.md): PostgreSQL, the backend, and your OIDC provider.
- [Architecture and protocol](PROTOCOL.md): process ownership, connections, and contracts.
- [Encryption design and threat model](CRYPTOGRAPHY.md): keys, rules, and known limits.
- [Contributing](../CONTRIBUTING.md): changes, issues, and review expectations.
- [Code of Conduct](../CODE_OF_CONDUCT.md): how we treat each other in this project.
- [About the pictures](media/README.md): what the pictures show and how to replace one.

## Product and native integration

[PRODUCT.md](../PRODUCT.md) is the source of product intent. The
[Mac](../clients/macos/PRODUCT.md) and [Linux](../clients/linux/PRODUCT.md) notes cover
platform behavior; their design references live beside them.

For terminal engine changes, read the
[Ghostty integration guide](../clients/macos/docs/GHOSTTY_INTEGRATION.md) and
[artifact provenance](../terminal/ghostty/PROVENANCE.md).

[Back to Kodosi](../README.md)
