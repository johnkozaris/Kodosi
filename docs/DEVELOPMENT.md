# Development

Keep these checkouts under `~/Repos`: `Kodosi`, `kodosiSwift`, `KodosiQT`, and
`kodosi-ghostty`. Root recipes use the adjacent Ghostty checkout unless
`KODOSI_GHOSTTY_DIR` is set. `KODOSI_SWIFT_DIR` selects a different Swift checkout for
lock parity checks.

## Builds and tests

Use the Rust toolchain in `rust-toolchain.toml` and the exact .NET SDK in
`dotnetBackend/global.json`. Install additional tools inside the repository, not
globally. `just rust-tools-install` places the pinned Cargo audit and coverage tools
in `.tools/rust`. The backend SDK can live in
`dotnetBackend/.tools/dotnet-10.0.401`; the root recipes put it on `PATH`.

```sh
just rust-build
just rust-test
just protocol-check
just rust-tools-install
just rust-all
just backend-test
```

`rust-all` checks native pins, formatting, both embedded and CLI feature builds,
documentation, Clippy, dependencies, the current desktop contract, tests, and the
55% line-coverage floor. `rust-coverage` and `rust-coverage-lcov` produce optional
reports. Test output and artifacts belong under `rustProcess/target`, not in source.
The gates do not automatically delete the build cache or install missing tools.

`just rust-ffi-build` builds the release static library;
`just rust-ffi-debug-build` uses the `ffi-debug` profile. cbindgen generates the C
header only in Cargo's artifact-specific `OUT_DIR`. The FFI recipes publish an
archive/header bundle under `rustProcess/target/ffi-artifacts/<target>/<profile>`
and print `current`, which atomically selects the complete pair. A slot belongs to
one source manifest and keeps only the current bundle. Swift consumes the selected
bundle under the publisher's lock before old bundles are removed; Qt owns its slot
through the build graph. Both use `scripts/build-ffi-artifact.py` with their own
build-local caches. There is no shared `target/include` header fallback.
Regenerate the desktop manifest with `just protocol-gen`; `protocol-check` is
read-only. There are no historical protocol compatibility snapshots.

The Rust build checks the Ghostty package commit, platform ref, and archive digest.
The Swift and Qt repositories own their final-link and client gates. Use their
explicit non-publishable working-tree validation mode for coordinated uncommitted
changes. Do not substitute invented release hashes or weaken native provenance.
A macOS build does not prove Linux-native behavior.

The backend solution contains one application and its tests. `backend-test` restores
locked dependencies, builds the solution, and runs Microsoft Testing Platform tests.
PostgreSQL integration tests require disposable storage and a working container
runtime. `just backend-run` starts `src/Kodosi.Host`. Development configuration uses
normal ASP.NET configuration, including `ConnectionStrings__Kodosi`. Run exactly one
serving backend process; relay presence and bounded replay are process-local.

Never point a test at a live database. An unfamiliar schema must fail rather than
trigger a reset. `backend-docker-down` preserves volumes; the root recipes provide no
volume-reset command.

## CLI

After `just rust-cli-build`, the executable is `rustProcess/target/debug/kodosi`.
`kodosi --help` and each subcommand's `--help` describe the current arguments.

```sh
kodosi host
kodosi session start --name Work --directory "$HOME/Repos/example"
kodosi session list
kodosi session attach <session-uuid>
kodosi session close <session-uuid>
```

Run `host` in a dedicated terminal when you want its lifetime to be explicit. It ends
its local processes when interrupted. Other commands connect to the local host or
start a background host if none is available. A background host exits on its own
after 30 seconds without terminals or clients, and yields immediately when the app
launches while it is idle. The app reports any other host by process id and kind
instead of starting; stopping a host that still runs terminals needs explicit
consent, and a host started with `kodosi host` is never stopped silently. In an
attached terminal, press `Ctrl+]` then `.` to detach without stopping the session.

On macOS the app executable is the CLI. `kodosi_cli_main` runs before AppKit when the
executable is invoked as `kodosi` or with a CLI subcommand, so a `kodosi` symlink to
`KodosiDesktop.app/Contents/MacOS/KodosiDesktop` runs with the app's signature,
provisioning profile, and Keychain access group. The data-protection Keychain needs a
console login session; hosts started over SSH or by launchd cannot sign in.

```sh
kodosi auth login
kodosi devices list
kodosi friends add <username>
kodosi session share <session-uuid> <friend-user-uuid>
kodosi session share <session-uuid>
kodosi mission create "Release"
kodosi session mission <session-uuid> <mission-uuid>
kodosi provider history claude "$HOME/Repos/example"
kodosi provider read claude <conversation-uuid> "$HOME/Repos/example"
kodosi session resume claude <conversation-uuid> "$HOME/Repos/example"
kodosi provider inspect copilot --directory "$HOME/Repos/example"
```

`session share` replaces the selected friend set; omit friend IDs to remove all
friend shares. Own approved devices retain access. Mission attachment does not share
a terminal. Missions use a name and UUID; no globally reserved short name is required.
Membership and invitation limits are checked before accepting new state, and an
oversized existing catalog returns a bounded recovery page with explicit truncation
indicators so users can leave or decline entries. Existing legacy
slug/profile values remain in the database; the slug migration relaxes constraints
without deleting values. `session input` waits until the hosting PTY has accepted the entire byte
sequence before reporting `submitted: true`; this does not mean the shell command
completed. Delivery timeouts are uncertain and must not be automatically retried.
Each command client owns its remote-view demand. Minimizing one desktop view releases
that client's demand, not another CLI viewer's connection; CLI detach releases its
own demand when its command connection closes. Terminal close carries the final
output sequence and drains queued output before the relay closes viewers.

The current desktop protocol is 42, local socket protocol is 17, and backend API is 14.
Relay protocol remains 13 and C ABI is 7. The terminal action is `session.close` (`close` on the relay),
with no separate Stop action. Upgrade hosts, clients and the backend together. Input
success still means complete byte delivery to the PTY, not command completion.

New PTYs advertise `TERM=xterm-256color`, `COLORTERM=truecolor`, and
`TERM_PROGRAM=Kodosi`, independent of the launching terminal. Inherited terminal
size and Claude parent-session markers are removed; provider credentials and native
permission settings are preserved. Kodosi does not inject agent/Mission identifiers.

## Isolated runtime validation

Use a disposable directory outside production storage and set both
`KODOSI_DATA_ROOT` and `KODOSI_PRODUCTION_DATA_ROOT`. The latter must name the actual
production root so overlap checks remain meaningful. The isolated runtime stores its
files in `<KODOSI_DATA_ROOT>/core` and rejects overlapping or symlinked storage paths.

The default production root is the platform configuration directory plus `kodosi`.
`HOME` supplies the default provider-state directories. `CLAUDE_CONFIG_DIR` and
`COPILOT_HOME`, when set to bounded absolute paths, select the same native state for
configuration, history and resume validation that the launched CLIs use. Provider
read APIs receive the resolved state root explicitly. For provider tests, use disposable
fixture roots and unset or redirect those overrides; preserve the actual production
Kodosi root explicitly. Never move, edit, or delete real provider settings, memory,
conversation histories or credentials to test discovery or resume.

Runtime connection settings are environment variables:

- `KODOSI__BACKEND__API`
- `KODOSI__AUTH__ISSUER` (default `https://auth.kodosi.com/realms/kodosi`, the Kodosi
  Keycloak realm), `KODOSI__AUTH__CLIENT_ID` (default `kodosi-app`, a public client
  using the device authorization grant), and `KODOSI__AUTH__SCOPE`
- `KODOSI__AUTH__KEYRING_SERVICE`
- `KODOSI__RUNTIME__INITIAL_SHELL`

Backend URLs require HTTPS except for localhost development. Provider inspection
locates native files and the executable without launching the provider. Resume is an
explicit process start and retains the provider's native prompts and permissions.

## Selecting database storage

Compose requires `KODOSI_POSTGRES_VOLUME` naming an existing, deliberately selected
Docker volume. It never creates a replacement application database implicitly.
Inspect and back up the existing volume before choosing it. PostgreSQL 18 expects
its versioned data directory under `/var/lib/postgresql`; a volume using the older
`/var/lib/postgresql/data` layout must be migrated explicitly with PostgreSQL tools.
Do not point the new backend at a live legacy schema expecting an automatic upgrade:
unsupported schemas are rejected without modifying their data. For disposable local
testing only, create a separately named volume and select it explicitly.
