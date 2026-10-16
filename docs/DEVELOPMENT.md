# Development

One checkout builds everything; [AGENTS.md](../AGENTS.md) lists the folders and
[Architecture](ARCHITECTURE.md) how they connect.

## Toolchains

Install `just`, Python 3 and the toolchains of the part you change. Rust is pinned in
[rust-toolchain.toml](../rust-toolchain.toml) and .NET in
[backend/global.json](../backend/global.json) (no roll-forward; put that SDK in
`backend/.tools/dotnet/`). `just rust-tools-install` puts the Rust check tools in
`.tools/rust/`. Keep tools and dependencies inside the project. Native prerequisites
are in the [Mac](../clients/macos/README.md),
[Linux and Windows](../clients/qt/README.md) and [iOS](../clients/ios/README.md) guides;
on Windows run the recipes in Git Bash.

## Commands

From the repository root; `just --list` shows them all.

| Command | Does |
| --- | --- |
| `just rust-cli-build` | Build the development CLI |
| `just rust-check` | All Rust checks |
| `just backend-check` | Build and test the backend (database tests need Docker) |
| `just check-all` | The Rust and backend checks |
| `just mac-build`, `just mac-check` | Build or check the Mac app |
| `just linux-build`, `just linux-check` | Build or check the Linux app |
| `just windows-check` | Build and check the Windows app, on Windows |
| `just windows-rust-lint` | Lint the runtime for Windows in Docker, from any host |
| `just ios-build`, `just ios-check` | Build or check the iPhone and iPad app |
| `just sign-in-check` | Check and build the sign-in pages |

Platform justfiles hold the release recipes.

## Names

- A recipe name ends with its verb: `build`, `test`, `lint` (changes no file), `format`,
  `check` (changes no source), `package`, `install`, `gen` (writes generated files),
  `run`, `configure`, `migrate`, `sign` or `stop`. Root recipes start with the component
  or tool (`mac-build`); component justfiles use the verb alone or subject and verb
  (`check`, `release-build`). `<component>-check` runs all checks of a component. Mark
  helper recipes `[private]`.
- A script starts with its verb (`build-ghostty.sh`, `generate-…` for `gen`); one that
  checks an artifact, pin or notice against its record starts with `verify-`. A script
  that prints a value, has subcommands or is sourced has a noun name (`digest.py`).
- Scripts use hyphens; Python modules and tests use underscores. Script tests live in
  the component's `scripts/tests/` as `test_<subject>.py` or `test-<subject>.sh`.
- Shared helpers have one name everywhere: `file_sha256`, `bytes_sha256`, `load_json`,
  `load_metadata`, `repository_root` (`kodosi_repository_root` in shell),
  `assert_rejected`, `fail`, `usage`.

## Run the backend

Set `ConnectionStrings__Kodosi` and the OIDC settings of [Self-hosting](SELF-HOSTING.md),
then `just backend-run`, which migrates first. Back up data before migrating. For a
local backend on `127.0.0.1:5180` that signs in with `auth.kodosi.com`, give
[compose.yaml](../compose.yaml) a new PostgreSQL volume:

```sh
docker volume create kodosi-docs-dev-postgres
export KODOSI_POSTGRES_VOLUME=kodosi-docs-dev-postgres
just backend-docker-run
```

`just backend-docker-stop` keeps the volume. Set `Auth__Authority` and `Auth__Audience`
in a local Compose override to use another identity provider, such as the disposable
Keycloak of `sign-in/keycloak.sh`.

## Contracts and Ghostty

`just protocol-gen` writes `protocol/desktop-runtime.json` from the Rust types and
`just protocol-check` checks it. After a change of the C API, copy the header that the
`kodosi-ffi-c` build writes to `clients/apple/Sources/KodosiKit/kodosi_runtime.h`.
[Ghostty.lock](../Ghostty.lock) pins Ghostty, `just ghostty-check` verifies it, and
[Ghostty provenance](../terminal/ghostty/PROVENANCE.md) explains updates.

## Data formats

Kodosi keeps no code for earlier formats: after a format change, use a new database
volume and data root. A device identity in an earlier format stops sign-in with "Stored
device identity is invalid": delete the item `kodosi.<user id>.device` of the service
`com.kodosi.local` (Keychain Access on macOS; `secret-tool clear service
com.kodosi.local username kodosi.<user id>.device` on Linux) and approve the computer
again.

## Isolated validation

Validate by running the app or calling the real boundary, with a disposable database and
a disposable `KODOSI_DATA_ROOT`; set `KODOSI_PRODUCTION_DATA_ROOT` to the real root so
that the runtime refuses an overlap. `KODOSI__RUNTIME__INITIAL_SHELL` picks the shell of
new terminals. Never touch real provider settings, memory, conversations, credentials or
working files. The app writes `app.log` and a CLI host `host.log` in the data root,
redacted; check them for private data before you attach them to an issue.
