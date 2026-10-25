# Development

[AGENTS.md](../AGENTS.md) lists the folders and [Architecture](ARCHITECTURE.md) shows how
they connect.

## Toolchains

Install `just`, Python 3 and the toolchains of the part you change. Rust is pinned in
[rust-toolchain.toml](../rust-toolchain.toml) and .NET in
[backend/global.json](../backend/global.json); put that .NET SDK in
`backend/.tools/dotnet/`. `just rust-tools-install` puts the Rust check tools in
`.tools/rust/`. Keep every tool and dependency inside the project. The
[Mac](../clients/macos/README.md), [Linux and Windows](../clients/qt/README.md) and
[iOS](../clients/ios/README.md) guides list the native prerequisites; on Windows run the
recipes in Git Bash.

## Commands

Run these from the repository root; `just --list` shows the rest.

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

`gitleaks git`, run by hand, looks for secrets in the Git history and skips the reviewed
findings in [.gitleaksignore](../.gitleaksignore).

The justfile in each app's folder has its release recipes. Name a new recipe component
first and verb last (`mac-build`), a script verb first (`build-ghostty.sh`), and other
files like their neighbors.

## Run the backend

Set `ConnectionStrings__Kodosi` and the sign-in settings of
[Self-hosting](SELF-HOSTING.md), then run `just backend-run`, which migrates first. Back
up data before migrating. For a local backend on `127.0.0.1:5180` that signs in with
`auth.kodosi.com`, give [compose.yaml](../compose.yaml) a new PostgreSQL volume:

```sh
docker volume create kodosi-dev-postgres
export KODOSI_POSTGRES_VOLUME=kodosi-dev-postgres
just backend-docker-run
```

`just backend-docker-stop` stops it and keeps the volume. To sign in with another
identity provider, such as the disposable Keycloak of `sign-in/keycloak.sh`, set
`Auth__Authority` and `Auth__Audience` in a local Compose override.

Debug builds of the Mac and iPhone apps use this local backend. For the Linux and
Windows app and the CLI, set `KODOSI__BACKEND__API=http://127.0.0.1:5180`, and
`KODOSI__AUTH__ISSUER` for another sign-in service.

## Contracts and Ghostty

`just protocol-gen` writes `protocol/desktop-runtime.json` from the Rust types and
`just protocol-check` checks it. After a change of the C API, copy the header that the
`kodosi-ffi-c` build writes to `clients/apple/Sources/KodosiKit/kodosi_runtime.h`.
`just ghostty-check` checks the Ghostty pins, and
[Ghostty provenance](../terminal/ghostty/PROVENANCE.md) explains updates.

## Isolated validation

Validate by running the app or calling the real boundary, with a disposable database and
a disposable `KODOSI_DATA_ROOT`. Set `KODOSI_PRODUCTION_DATA_ROOT` to the real data root
too, so the runtime refuses an overlap. `KODOSI__RUNTIME__INITIAL_SHELL` picks the shell
of new terminals. The Mac app gives its host `<KODOSI_DATA_ROOT>/harness-home` as `HOME`;
elsewhere terminals get your real `HOME`, so point the shell variable at a script that
sets a disposable one. Never touch real provider settings, memory, conversations,
credentials or working files. The app writes a redacted `app.log` in the data root, and
the host `host.log`; check them for private data before you attach them to an issue.
With `KODOSI_TERMINAL_DEBUG=1`, the Mac and iPhone apps write the sizes and lifecycle of
their terminal views to stderr.

## Data formats

[AGENTS.md](../AGENTS.md) says when a format can change without a reader for the old one.
A change of the database is a new migration (`dotnet ef migrations add <Name>` in
`backend/`). A contract change that older apps cannot use raises the oldest version in
`protocol/` in the same change.
