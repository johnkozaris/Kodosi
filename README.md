# Kodosi

Kodosi runs real terminals on your computer and lets you use them from your other
approved devices or share them with trusted friends. Execution stays on the host.
Remote terminal traffic is encrypted between endpoint devices; the backend stores
account, device, sharing, and Mission metadata and relays ciphertext.

- Start a shell, attach to a running terminal, interrupt it, or stop it.
- Share selected sessions with friends. Everyone admitted has full terminal control,
  including Stop. Friendship alone does not grant access.
- Use Missions to organize people and attached terminals. Joining a Mission does not
  grant access to its terminals.
- Preview saved Claude Code or Copilot CLI conversations and resume them with the
  installed provider's native CLI. Open the provider's own configuration files.

Closing a terminal view leaves its process running. Stop ends the process. Quitting
an app that hosts local sessions ends those processes. Saved provider conversations
remain separate from live sessions.

## Repository

- `rustProcess/`: shared runtime, command-line client, and C ABI for arm64 macOS and
  x86_64 Linux.
- `dotnetBackend/`: one ASP.NET application, PostgreSQL metadata, and WebSocket relay.
  Run one serving backend process.
- `protocol/`: current cross-stack contracts. Rust generates the desktop contract.
- `../kodosiSwift`: native macOS client.
- `../KodosiQT`: native Linux Qt client.
- `../kodosi-ghostty`: pinned native renderer and headless terminal engine.

See [PRODUCT.md](PRODUCT.md) for the product boundary and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for builds, tests, CLI usage, and isolated
validation. Cross-user identity bootstrap uses trust on first use; an active backend
substitution before the first identity pin is outside the current threat model.
