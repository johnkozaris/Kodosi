# Kodosi

Kodosi runs terminals on your computer and lets you use them from approved devices or
share selected terminals with other people. The product direction is rooms where
people and agents share terminals and a conversation.

See [PRODUCT.md](PRODUCT.md) for intended behavior and current implementation gaps, and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for working on the repository.

## Repository

- `runtime/`: terminal runtime, CLI, identity, encrypted transport, and native API.
- `backend/`: shared metadata and live encrypted terminal connections.
- `protocol/`: current generated cross-stack contracts.
- `../KodosiMac`: native macOS client.
- `../KodosiUI`: native Linux Qt client.
- `../kodosi-ghostty`: pinned terminal engine.

## License

MIT. See [LICENSE](LICENSE).
