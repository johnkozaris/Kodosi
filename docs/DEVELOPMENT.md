# Development

Clone `Kodosi` under `~/Repos`. The runtime, backend, native clients and Ghostty
integration live in this checkout. No sibling repositories or submodules are needed.

## Builds and tests

Use the root `justfile`. `just --list` is the current command reference. The main
gates are:

```sh
just rust-all
just backend-test
just check-all
just mac-build    # Apple silicon macOS with Xcode
just mac-check
just linux-build  # Linux x86-64
just linux-check
```

Use the toolchains selected by `rust-toolchain.toml` and
`backend/global.json`. Install extra tools through the repository recipes so
they stay project-local. Build artifacts belong in their normal project output
directories, not in source.

Rust owns the generated desktop contract and C header. Use `just protocol-gen` when
the contract intentionally changes and `just protocol-check` to verify committed
output. Do not maintain historical contract copies.

The clients compile the runtime from this checkout. Root `Ghostty.lock` selects the
upstream revisions and native artifact digests in `terminal/ghostty/`. Its build
metadata and third-party notices retain the evidence for those artifacts. Follow
[Ghostty provenance](../terminal/ghostty/PROVENANCE.md) for native updates.

Mac setup and release commands are in [clients/macos](../clients/macos/README.md).
Linux bootstrap installs its pinned Qt, CMake and Ninja under
`clients/linux/.tools/`. Platform-specific commands also remain available from each
client directory. Release packages record the single Kodosi source revision.

Run the backend with `just backend-run`. It applies the database schema with
`just backend-migrate` first; a serving start does not change the schema and refuses a
database that is not current. Run one serving backend process at a time.
PostgreSQL tests require disposable storage and a working container runtime.

Build the CLI with `just rust-cli-build`. Use `kodosi --help` and subcommand help for
the current command surface instead of copying it into documentation.

`kodosi room skill` prints the bundled agent instructions. Room commands use the
running host's sign-in and infer the room from `KODOSI_SESSION_ID` inside a shared
terminal. Use `--room` to select one explicitly.

GitHub issue actions use the local `gh` sign-in or Git credential helper. Gitea uses
the Git credential helper, or a `KODOSI_GITEA_TOKEN` scoped to the origin named by
`KODOSI_GITEA_URL`. The account needs issue access; read-only credentials can browse
but cannot change issues. These credentials are never sent to the Kodosi backend.

## Isolated runtime validation

Never point tests at live storage. Use a disposable database and a disposable
`KODOSI_DATA_ROOT`. Set `KODOSI_PRODUCTION_DATA_ROOT` to the real production root so
the runtime can reject accidental overlap. Unknown database schemas must fail without
resetting or modifying them.

The runtime writes its diagnostics to `diagnostics.log` in its data root. It keeps one
older copy. Local terminals never show these background failures.

Provider tests must use fixture directories or redirected native configuration roots.
Never move, edit, or delete real provider settings, memory, conversations,
credentials, or working files.

Select PostgreSQL volumes explicitly. Inspect and back up existing data before any
migration, and use separately named disposable volumes for tests.
