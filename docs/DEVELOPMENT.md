# Development

One checkout contains the runtime, backend, the native clients, generated contracts,
and Ghostty integration. No sibling repositories or submodules are needed.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `runtime/` | Rust runtime, CLI, terminal processes, identity, encryption, native API |
| `backend/` | ASP.NET Core metadata service and encrypted traffic relay |
| `clients/macos/` | Swift native Mac app |
| `clients/qt/` | Qt native Linux and Windows app |
| `clients/ios/` | Swift native iPhone and iPad app |
| `clients/apple/` | Swift package shared by the Mac and iOS apps |
| `protocol/` | Generated contracts shared across the stack |
| `terminal/ghostty/` | Native terminal integration, patches, artifacts, and notices |
| `terminal/conpty/` | Pinned Windows console host for terminals on Windows |

See [Architecture and protocol](PROTOCOL.md) for how the processes connect.

## Toolchains

Install `just`, Python 3, and the toolchains needed for the component you are changing.
The repository pins Rust in [rust-toolchain.toml](../rust-toolchain.toml) and .NET in
[backend/global.json](../backend/global.json). Respect those versions; the .NET SDK
does not roll forward automatically.

`just rust-tools-install` installs the Rust check tools under `.tools/rust/`.
`just bacon-install` installs these tools and Bacon. To run the jobs in
[bacon.toml](../bacon.toml) again after each change to a file, run
`.tools/rust/bin/bacon` from the repository root.
The root recipes also recognize a local .NET SDK at `backend/.tools/dotnet-10.0.401/`.
Keep dependencies and downloaded build tools inside the project.

For native prerequisites and signing, follow the
[Mac build guide](../clients/macos/README.md), the
[Linux and Windows build guide](../clients/qt/README.md), or the
[iOS build guide](../clients/ios/README.md). Build each app on its own platform; the
iOS app builds on a Mac. On Windows, run the recipes in Git Bash.

## Build and check

Run these from the repository root. `just --list` is the complete command reference.

| Command | Purpose |
| --- | --- |
| `just rust-cli-build` | Build the standalone development CLI |
| `just rust-check` | Ghostty pins and notices, Rust format, compilation, docs, lint, dependencies, contracts, tests, coverage |
| `just backend-test` | Build and test the backend; database tests need a container runtime |
| `just check-all` | Run the Rust and backend checks |
| `just mac-build` / `just mac-check` | Build / check the Mac app |
| `just linux-build` / `just linux-check` | Build / check the Linux app |
| `just windows-check` | Build and check the Windows app on Windows |
| `just windows-rust-lint` | Lint the runtime for both Windows targets in Docker, from any host |
| `just ios-build` / `just ios-check` | Build / check the iPhone and iPad app |
| `just ios-rust-targets-install` / `just ios-rust-lint` | Install / lint the iOS Rust targets |

Platform directories retain their own `just` recipes, including release packaging.
Artifacts stay in the component's output directories. Release packages record the
single Kodosi source revision.

## Names and folders

Use these rules for the names of files, folders, recipes, scripts and helper
functions. Use one word for one meaning.

### Folders and files

- Write `justfile` and `scripts/` in lowercase.
- Put the tests for the scripts of a component in the `scripts/tests/` folder of that
  component.
- Name a Python test `test_<subject>.py` and a shell test `test-<subject>.sh`. The
  subject is the item that the test examines. For example, `test-rust-archive.sh`
  tests `verify-rust-archive.sh`.
- To find all the tests of the scripts, run `git ls-files '*scripts/tests/test[-_]*'`.
- Name a Dockerfile after the image that it makes, for example
  `windows-cross.Dockerfile` for the image `kodosi-windows-cross`.

### Recipes

A recipe name ends with one of these verbs:

| Verb | Meaning |
| --- | --- |
| `build` | Make a program, a library or the documentation from the source. |
| `test` | Run tests. |
| `lint` | Examine the source with a linter. Change no file. |
| `format` | Change the source to the standard format. |
| `check` | Examine the source or the output for errors. Change no source file. |
| `package` | Make a package of built files, for example a Debian package or a disk image. |
| `install` | Put a pinned tool, a package or a built file where the next step uses it. |
| `gen` | Write generated files, for example the contracts or a coverage report. |
| `run` | Start a program. |

Four verbs have only one subject: `configure` (the CMake build), `migrate` (the
backend database), `sign` (a release file) and `stop` (the Docker Compose services).

- In the root `justfile`, start a recipe name with the component or the tool that it
  is for, for example `mac-build`, `rust-format-check` or `bacon-install`.
- In the `justfile` of a component, use the verb alone, or a subject and then the verb,
  for example `check` or `release-build`.
- `<component>-check` in the root, and `check` in the `justfile` of a component, run all
  the checks of that component.
- `check-all` is the only recipe that runs the checks of more than one component. It
  is also the only name that starts with its verb.
- Mark a recipe that only other recipes use with `[private]`.

### Scripts

- Start the name of a script that does one task with its verb, for example
  `build-ghostty.sh` or `install-tools.sh`. Use the verbs of the recipe table with the
  same meaning. A script name uses `generate-`, and a recipe name uses its short form
  `gen`.
- Start the name of a script that examines an artifact, a pin or a set of notices with
  `verify-`. The script compares the item with its record. It stops with an error
  when they are different. Do not use `verify` in a recipe name. Use `check`.
- Use another verb only when no verb in the table has its meaning, for example
  `apply-patches.sh` or `notarize.sh`.
- Give a noun name to a script that only writes a value, for example
  `artifact-digest.sh`. Also give a noun name to a script with subcommands, for
  example `metal-toolchain.sh mount`.
- Give a noun name to a shell file that other scripts source, for example
  `kodosi-paths.sh`. Start the names of its functions with `kodosi_`.
- Use hyphens in the names of scripts. Use underscores in the names of Python tests
  and of Python files that other files import, for example `release_common.py`.
  Python cannot import a name with a hyphen.

### Helper functions

Give the same helper the same name in each script:

| Helper | Job |
| --- | --- |
| `file_sha256` | Give the SHA-256 of a file. |
| `bytes_sha256` | Give the SHA-256 of bytes. |
| `load_json` | Read a JSON object from a file. |
| `load_metadata` | Read the `key=value` record of an artifact. |
| `kodosi_repository_root` (shell), `repository_root` (Python) | Give the root folder of the repository. |
| `assert_rejected` | Make a test fail when a check accepts bad input. |
| `fail` | Write an error message and stop. |
| `usage` | Write the usage text. |

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
just backend-docker-run
```

`just backend-docker-stop` stops the services without removing database storage.
For your own identity provider, add `Auth__Authority` and `Auth__Audience` to the
backend service's environment through a local Compose override.

## Contracts and native artifacts

Rust owns the desktop contract and C header. Use `just protocol-gen` when a contract
intentionally changes, and `just protocol-check` to check the committed output.
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
approve the computer again as a new one. It is the item `kodosi.<user id>.device`
of the service `com.kodosi.local`. On macOS, delete it in Keychain Access. On Linux, run
`secret-tool clear service com.kodosi.local username kodosi.<user id>.device`.

## Isolated validation

Validate a workflow by running the app or calling the real boundary and inspecting
the result. Use a disposable database and a disposable `KODOSI_DATA_ROOT`; set
`KODOSI_PRODUCTION_DATA_ROOT` to the real production root so the runtime can reject
overlap. These variables are for development isolation, not a server profile UI.

Provider tests use fixtures or redirected native configuration roots. Never move,
edit, or delete real provider settings, memory, conversations, credentials, or working
files. Select PostgreSQL volumes explicitly; unknown schemas must fail without a reset.

Each process writes its own redacted log in the data root and keeps one older copy:
the app writes `app.log` and a CLI host writes `host.log`. The Qt app adds its Qt
messages, except debug output, to `app.log`. Inspect logs locally and check for private
data before attaching them to an issue.

[All documentation](README.md) · [Contributing](../CONTRIBUTING.md)
