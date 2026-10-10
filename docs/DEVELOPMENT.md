# Development

Clone Kodosi under `~/Repos`. One checkout contains the runtime, backend, both
native clients, generated contracts, and Ghostty integration. No sibling repositories
or submodules are needed.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `runtime/` | Rust runtime, CLI, terminal processes, identity, encryption, native API |
| `backend/` | ASP.NET Core metadata service and encrypted traffic relay |
| `clients/macos/` | Swift native Mac app |
| `clients/linux/` | Qt native Linux app |
| `protocol/` | Generated contracts shared across the stack |
| `terminal/ghostty/` | Native terminal integration, patches, artifacts, and notices |

See [Architecture and protocol](PROTOCOL.md) for how the processes connect.

## Toolchains

Install `just`, Python 3, and the toolchains needed for the component you are changing.
The repository pins Rust in [rust-toolchain.toml](../rust-toolchain.toml) and .NET in
[backend/global.json](../backend/global.json). Respect those versions; the .NET SDK
does not roll forward automatically.

`just rust-tools-install` installs the Rust checking tools under `.tools/rust/`.
The root recipes also recognize a local .NET SDK at `backend/.tools/dotnet-10.0.401/`.
Keep dependencies and downloaded build tools inside the project.

For native prerequisites and signing, follow the
[Mac build guide](../clients/macos/README.md) or
[Linux build guide](../clients/linux/README.md). Build each app on its own platform.

## Build and check

Run these from the repository root. `just --list` is the complete command reference.

| Command | Purpose |
| --- | --- |
| `just rust-cli-build` | Build the standalone development CLI |
| `just rust-all` | Rust format, compilation, docs, lint, dependencies, contracts, tests, coverage |
| `just backend-test` | Build and test the backend; database tests need a container runtime |
| `just check-all` | Run the Rust and backend gates |
| `just mac-build` / `just mac-check` | Build / check the Mac app |
| `just linux-build` / `just linux-check` | Build / check the Linux app |

Platform directories retain their own `just` recipes, including release packaging.
Artifacts stay in the component's output directories. Release packages record the
single Kodosi source revision.

## Run the backend

Configure `ConnectionStrings__Kodosi` for a PostgreSQL database you intend to use,
plus the OIDC settings described in [Self-hosting](SELF-HOSTING.md). Then run:

```sh
just backend-run
```

This recipe runs `backend-migrate` before serving. A normal serving start does not
change the schema and refuses a database that is not current. Run one serving backend
process. Inspect and back up existing data before applying migrations.

The root [compose.yaml](../compose.yaml) is a local backend setup. It binds to
`127.0.0.1:5180`, uses a development database password, and requires an explicitly
selected PostgreSQL volume. It does not run an identity provider. For a new local
database, choose a volume name that is not in use:

```sh
docker volume create kodosi-docs-dev-postgres
export KODOSI_POSTGRES_VOLUME=kodosi-docs-dev-postgres
just backend-docker-up
```

`just backend-docker-down` stops the services without removing database storage.
For your own identity provider, add `Auth__Authority` and `Auth__Audience` to the
backend service's environment through a local Compose override.

## Contracts and native artifacts

Rust owns the desktop contract and C header. Use `just protocol-gen` when a contract
intentionally changes, and `just protocol-check` to verify the committed output.
Keep one current contract set under [protocol/](../protocol/).

The clients compile the runtime from this checkout. Root [Ghostty.lock](../Ghostty.lock)
selects the upstream revisions and artifact digests under `terminal/ghostty/`.
Follow [Ghostty provenance](../terminal/ghostty/PROVENANCE.md) for native updates;
commit measured build evidence, notices, pins, and affected consumers together.

## Data formats

Kodosi has no release yet, so it keeps no code for earlier protocol versions or saved
formats. After a format change, rooms and records in an existing development database
may not open: use a new database volume and data root. A device identity saved in an
earlier format stops sign-in with "Stored device identity is invalid". Delete it, then
approve the computer again as a new one. It is the item `terminal-first.<user id>.device`
of the service `com.kodosi.local`. On macOS, delete it in Keychain Access. On Linux, run
`secret-tool clear service com.kodosi.local username terminal-first.<user id>.device`.

## Isolated validation

Validate a workflow by running the app or calling the real boundary and inspecting
the result. Use a disposable database and a disposable `KODOSI_DATA_ROOT`; set
`KODOSI_PRODUCTION_DATA_ROOT` to the real production root so the runtime can reject
overlap. These variables are for development isolation, not a server profile UI.

Provider tests use fixtures or redirected native configuration roots. Never move,
edit, or delete real provider settings, memory, conversations, credentials, or working
files. Select PostgreSQL volumes explicitly; unknown schemas must fail without a reset.

The runtime writes `diagnostics.log` in its data root and keeps one older copy.
Inspect logs locally and redact private data before attaching them to an issue.

[All documentation](README.md) · [Contributing](../CONTRIBUTING.md)
