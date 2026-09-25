# Kodosi

Kodosi runs terminals on your computer and lets you use them from approved devices or
share selected terminals with trusted friends. Missions organize people and terminals
without granting access.

See [PRODUCT.md](PRODUCT.md) for the product boundary and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for working on the repository.

## Repository

- `runtime/`: terminal runtime, CLI, identity, encrypted transport, and native API.
- `backend/`: shared metadata and live encrypted terminal connections.
- `protocol/`: current generated cross-stack contracts.
- `../kodosiSwift`: native macOS client.
- `../KodosiQT`: native Linux Qt client.
- `../kodosi-ghostty`: pinned terminal engine.
