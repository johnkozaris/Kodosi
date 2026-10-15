# Development

One checkout builds everything; see [AGENTS.md](../AGENTS.md) for the folders and
[Architecture](ARCHITECTURE.md) for how they connect.

## Toolchains

Install `just`, Python 3 and the toolchains of the component you change. Rust is pinned
in [rust-toolchain.toml](../rust-toolchain.toml) and .NET in
[backend/global.json](../backend/global.json); the .NET SDK does not roll forward. Put
that SDK in `backend/.tools/dotnet/`, where the recipes find it. `just rust-tools-install`
puts the Rust check tools in `.tools/rust/`. Keep tools and dependencies inside the
project.

Native prerequisites are in the [Mac](../clients/macos/README.md),
[Linux and Windows](../clients/qt/README.md) and [iOS](../clients/ios/README.md) guides.
Build each app on its own platform; on Windows run the recipes in Git Bash.

## Commands

Run these from the repository root; `just --list` shows them all.

| Command | Does |
| --- | --- |
| `just rust-cli-build` | Build the development CLI |
| `just rust-check` | All Rust checks: Ghostty pins, format, lint, docs, dependencies, contracts, tests, coverage |
| `just backend-check` | Build and test the backend (database tests need Docker) |
| `just check-all` | The Rust and backend checks |
| `just mac-build`, `just mac-check` | Build or check the Mac app |
| `just linux-build`, `just linux-check` | Build or check the Linux app |
| `just windows-check` | Build and check the Windows app, on Windows |
| `just windows-rust-lint` | Lint the runtime for Windows in Docker, from any host |
| `just ios-build`, `just ios-check` | Build or check the iPhone and iPad app |
| `just sign-in-check` | Check and build the sign-in pages |

Platform justfiles hold the release recipes. Release packages record the source revision.

## Names

One word per meaning.

- A recipe name ends with its verb: `build`, `test`, `lint` (changes no file), `format`,
  `check` (changes no source), `package`, `install`, `gen` (writes generated files) or
  `run`. `configure`, `migrate`, `sign` and `stop` each have one subject.
- Root recipes start with the component or tool (`mac-build`, `rust-format-check`).
  Component justfiles use the verb alone or subject and verb (`check`,
  `release-build`). `<component>-check` and `check` run all checks of a component;
  `check-all` is the only recipe across components. Mark helper recipes `[private]`.
- A script starts with its verb (`build-ghostty.sh`), using `generate-` where a recipe
  says `gen`. A script that checks an artifact, pin or notice against its record starts
  with `verify-`. A script that only prints a value, has subcommands, or is sourced has
  a noun name (`digest.py`, `apple-platform.sh`, whose functions start with `kodosi_`).
- Script names use hyphens; Python modules and tests use underscores. Script tests live
  in the component's `scripts/tests/` as `test_<subject>.py` or `test-<subject>.sh`.
- Name a Dockerfile after its image, as `windows-cross.Dockerfile`.
- Shared helpers have one name everywhere: `file_sha256`, `bytes_sha256`, `load_json`,
  `load_metadata`, `repository_root` (`kodosi_repository_root` in shell),
  `assert_rejected`, `fail`, `usage`.

## Run the backend

Set `ConnectionStrings__Kodosi` to a PostgreSQL database and the OIDC settings in
[Self-hosting](SELF-HOSTING.md), then run `just backend-run`. It applies migrations
first; a normal start refuses a database whose schema is not current. Back up existing
data before migrating, and run one serving process.

[compose.yaml](../compose.yaml) runs a local backend on `127.0.0.1:5180` with a
development password and no identity provider. Give it a PostgreSQL volume; for a new
database, pick a name that is not in use:

```sh
docker volume create kodosi-docs-dev-postgres
export KODOSI_POSTGRES_VOLUME=kodosi-docs-dev-postgres
just backend-docker-run
```

`just backend-docker-stop` stops it and keeps the volume. Add `Auth__Authority` and
`Auth__Audience` through a local Compose override to use your own identity provider.

## Contracts and Ghostty

`just protocol-gen` writes `protocol/desktop-runtime.json` from the Rust types and
`just protocol-check` checks it. The `kodosi-ffi-c` build writes the C header; after a
change of the C API, copy it to `clients/apple/Sources/KodosiKit/kodosi_runtime.h` (the
Apple build shows the difference). The clients build the runtime from this checkout.
[Ghostty.lock](../Ghostty.lock) pins the Ghostty revisions and artifacts, and
`just ghostty-check` verifies them; follow
[Ghostty provenance](../terminal/ghostty/PROVENANCE.md) to update them.

## Data formats

Kodosi has no release and keeps no code for earlier formats. After a format change, use
a new database volume and data root. A device identity in an earlier format stops
sign-in with "Stored device identity is invalid": delete the item
`kodosi.<user id>.device` of the service `com.kodosi.local` (Keychain Access on macOS;
`secret-tool clear service com.kodosi.local username kodosi.<user id>.device` on Linux)
and approve the computer again.

## Isolated validation

Validate a workflow by running the app or calling the real boundary. Use a disposable
database and a disposable `KODOSI_DATA_ROOT`, and set `KODOSI_PRODUCTION_DATA_ROOT` to
the real root so that the runtime refuses an overlap. `KODOSI__RUNTIME__INITIAL_SHELL`
chooses the shell of new terminals, for example `/bin/sh`. Provider tests use fixtures or
redirected configuration roots; never touch real provider settings, memory,
conversations, credentials or working files.

The app writes `app.log` and a CLI host writes `host.log` in the data root, redacted,
with one older copy each. Check logs for private data before you attach them to an issue.
