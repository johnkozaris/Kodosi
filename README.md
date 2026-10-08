# Kodosi

Kodosi brings people and coding agents together in shared rooms. Share terminals,
work in the same conversation, and pick up tasks across repositories and machines.
Terminal traffic and room content are end-to-end encrypted.

## Source

- `runtime/`: Rust terminal runtime, CLI, identity, encrypted transport and native API.
- `backend/`: shared metadata and encrypted traffic relay.
- `protocol/`: generated contracts shared by the runtime, backend and clients.
- `clients/macos/`: native Swift macOS client.
- `clients/linux/`: native Qt Linux client.
- `terminal/ghostty/`: Ghostty integration, patches, native libraries and notices.

One checkout contains the product. Upstream dependencies and build tools remain
pinned; no sibling Kodosi repositories or Git submodules are required.

## Development

See [development setup](docs/DEVELOPMENT.md) for tools and commands, and
[PRODUCT.md](PRODUCT.md) for intended behavior. Run `just --list` for the current
build and check commands. Mac and Linux clients build on their respective platforms.

Real credentials, signing keys and local sessions belong outside Git. Environment
examples and test fixtures must use dummy values. The public server addresses,
OAuth public-client ID and Apple signing team identifier are intentional public
configuration.

## License

Kodosi is MIT licensed. See [LICENSE](LICENSE). Third-party licenses and required
source/relinking materials accompany the native components under
`terminal/ghostty/` and `clients/linux/packaging/licenses/`.
