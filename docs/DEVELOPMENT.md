# Development

See [AGENTS.md](../AGENTS.md) for the repository layout and [Architecture](ARCHITECTURE.md) for
how the subsystems connect.

## Toolchains

Install `just`, Python 3.12+, and the toolchains for the areas you work on. Keep all tools and
dependencies local to the repository:

- Rust is pinned in [rust-toolchain.toml](../rust-toolchain.toml). Run `just rust-tools-install`
  to install `cargo-deny` and `cargo-machete` into `.tools/rust/`.
- .NET is pinned in [backend/global.json](../backend/global.json); place that SDK in
  `backend/.tools/dotnet/`.
- Bun is pinned in [.bun-version](../.bun-version) and is required for document views and sign-in.
  The recipes and app builds install it into `.tools/bun/` when they need it.
- The sign-in Keycloak extension builds with Maven inside Docker, caching downloads in
  `.tools/maven/`.
- Native client prerequisites live in the [macOS](../clients/macos/README.md),
  [Linux and Windows](../clients/qt/README.md), [iOS](../clients/ios/README.md), and
  [Android](../clients/android/README.md) READMEs. On Windows, run `just` recipes in Git Bash.

Cargo and Bun enforce a minimum publish-age cooldown on new package releases (configured in
[.cargo/config.toml](../.cargo/config.toml) and each `bunfig.toml`) so compromised releases are
caught before Kodosi picks them up. To take an urgent fix immediately, run the Cargo or the Bun
command:

```sh
CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow cargo update -p <crate> --precise <version>
bun add --minimum-release-age=0 <package>@<version>
```

## Commands

Run from the repository root (`just --list` shows every available recipe):

| Command | Purpose |
| --- | --- |
| `just rust-cli-build` | Build the dev `kodosi` CLI |
| `just agent-browser-build` | Build the pinned `agent-browser` binary next to the dev CLI |
| `just documents-check` | Build, lint, and test document views and Rust crates |
| `just documents-app-check` | Format-check and lint the standalone Tauri documents app |
| `just documents-app-run <files>` | Launch the standalone documents app with one window per file |
| `just browser-check` | Format-check, lint, and test the shared browser crate and engine |
| `just browser-engine-build` | Build Kodosi Browser for development into `.tools/kodosi-browser/` |
| `just rust-check` | Run all Rust formatting, Clippy, dependency, Ghostty, and test checks |
| `just backend-check` | Verify the .NET SDK pin, build, and run backend tests (DB tests require Docker) |
| `just check-all` | Run Rust, documents, browser, backend, and translation checks |
| `just mac-build`, `just mac-check` | Build or fully check the macOS app |
| `just linux-build`, `just linux-check` | Build or fully check the Linux app |
| `just windows-check` | Build and check the Windows app (run on Windows) |
| `just windows-rust-lint` | Cross-lint the Windows Rust runtime in Docker from any host |
| `just ios-build`, `just ios-check` | Build or fully check the iOS app |
| `just android-build`, `just android-check` | Build or fully check the Android app |
| `just sign-in-check` | Lint and build the sign-in theme, and build and test its Keycloak extension (requires Docker) |
| `just translations-check` | Verify every user-facing string is translated across all languages |

Run `gitleaks git` manually to scan Git history for leaked secrets (reviewed false positives live
in [.gitleaksignore](../.gitleaksignore)). Release packaging recipes live in each client's own
`justfile`.

## Before a pull request

Run the checks covering everything you touched:

| Area changed | Command |
| --- | --- |
| Runtime, protocol contract, or Rust dependency | `just rust-check` |
| Live documents or shared browser | `just documents-check`, `just documents-app-check`, `just browser-check` |
| Backend | `just backend-check` |
| Sign-in theme or extension | `just sign-in-check` |
| Any user-facing string | `just translations-check` |
| Platform-gated (`cfg`) Rust runtime code | `just windows-rust-lint` (requires Docker) |
| macOS, iOS, or shared Apple code | `just mac-check`, `just ios-check` |
| Linux / Windows Qt app | `just linux-check` (Linux) or `just windows-check` (Windows) |
| Android app | `just android-check` |
| Update-site or engine-feed release scripts | `just release-test` |

Pull request CI only re-runs the first five rows for fast feedback and does not build client apps.
Nightly builds on `main` build the Linux app, Windows app, and Windows package. If you edited
Linux/Windows client code without access to that OS (or platform-specific Rust code without
Docker), trigger the `Linux app` or `Windows app` workflow manually against your branch and wait
for it to pass:

```sh
gh workflow run "Linux app" --ref <branch>
```

## Run the backend

Set `ConnectionStrings__Kodosi` and the OIDC settings from [Self-hosting](SELF-HOSTING.md), then
run `just backend-run` (which runs database migrations before starting; back up existing data
first).

To spin up a local backend at `127.0.0.1:5180` authenticating against `auth.kodosi.com`, create a
named PostgreSQL volume for [compose.yaml](../compose.yaml):

```sh
docker volume create kodosi-dev-postgres
export KODOSI_POSTGRES_VOLUME=kodosi-dev-postgres
just backend-docker-run
```

`just backend-docker-stop` stops the containers while preserving the volume. Upgrading to a new
major PostgreSQL version in `compose.yaml` requires a new volume (the old volume stays untouched).
To use a different identity provider, such as the disposable Keycloak started by
`sign-in/keycloak.sh`, override `Auth__Authority` and `Auth__Audience` in a local Compose override
and copy the account-deletion client credentials printed by `sign-in/keycloak.sh` into the
`Auth__AccountDeletion` settings described in [Self-hosting](SELF-HOSTING.md).

Debug builds of the macOS, iOS, and Android apps automatically target `127.0.0.1:5180` and the
disposable Keycloak from `sign-in/keycloak.sh` (overridable via `KODOSI__AUTH__ISSUER`). For the
Qt app and CLI, set `KODOSI__BACKEND__API=http://127.0.0.1:5180` (and `KODOSI__AUTH__ISSUER` if
using another issuer).

## Contracts and Ghostty

Run `just protocol-gen` to regenerate `protocol/desktop-runtime.json` from the Rust definitions;
Rust tests verify it stays in sync. After changing the C FFI in `kodosi-ffi-c`, copy the generated
header from the build output to `clients/apple/Sources/KodosiKit/kodosi_runtime.h`. Run
`just ghostty-check` to verify Ghostty pins, digests, and notices; see
[terminal/ghostty/PROVENANCE.md](../terminal/ghostty/PROVENANCE.md) for upgrade steps.

[AgentBrowser.lock](../AgentBrowser.lock) pins the `agent-browser` version and Git commit bundled
alongside the `kodosi` binary. App builds compile it into `.tools/agent-browser/` using its own
`Cargo.lock`. When updating the pin, verify the release tag, update both fields, rebuild each app,
and re-run `just agent-browser-build` (`just rust-cli-build` does not rebuild it).

### Kodosi Browser engine

Kodosi Browser links against prebuilt CEF pinned by `cef-dll-sys` (downloaded and verified at build
time, so Chromium is never compiled from source):

- `browser/build/build-engine.py --output-dir <folder>` builds the engine using `uv` and writes its
  archive and feed metadata.
- `scripts/generate-engine-feed.py <folder>` aggregates all platform archives into the signed feed
  (reading the signing key from stdin) hosted on the update site.
- In development, set `KODOSI_BROWSER_ENGINE` to the built engine binary to bypass the update feed.
  After changing code under `browser/`, re-run `just browser-engine-build` because no other recipe
  rebuilds it.
- Host and engine must agree on `VERSION` in `browser/crate/src/link.rs`. Feed URLs and engine
  builds are keyed by that protocol version, so hosts only fetch and connect to compatible engines,
  allowing app and engine releases to ship independently. Bump `VERSION` whenever the link protocol
  changes, and bump `browser/engine/Cargo.toml` on every engine change so hosts recognize the
  newer build in the feed.

## Texts and languages

Write every new user-facing string in English in code, then translate it into German, Greek,
Spanish, French, Italian, Japanese, Korean, and Simplified Chinese:

- Swift: `Text("…")` and `String(localized: "…")` (passing `bundle: .module` in shared packages).
  After building macOS or iOS, run `just apple-strings-sync` to extract new keys into the String
  Catalogs.
- Qt: `qsTr()` in QML and `tr()` in C++, using `%n` for plurals. Run `just linux-strings-sync` to
  update `clients/qt/i18n/`.
- Android: `stringResource(R.string.…)` in Compose, with entries in every
  `clients/android/app/src/main/res/values*/strings.xml`.
- Runtime: `text!("… {name} …", name = value)` for user-facing messages, with translations in
  `runtime/texts/`. Internal fault details stay English via `Text::plain`.
- Backend: user-facing rejection messages stay English in C# and are mapped to localized templates
  in `backend_refusals()` inside `runtime/src/network/http.rs` (with translations in
  `runtime/texts/`).
- Document views: `say("…")` from `@documents/words`, with translations in
  `documents/views/words/src/`.
- Sign-in theme: `sign-in/src/login/i18n.ts`, `sign-in/src/account/words.ts`, and
  `sign-in/src/email/messages/`.

`just translations-check` flags missing translations or mismatched format placeholders.
`just mac-check`, `just ios-check`, and `just linux-check` also fail if String Catalogs or Qt `.ts`
files are missing strings from the source.

## Isolated validation

Always test against real boundaries using a disposable database and a disposable
`KODOSI_DATA_ROOT`. Also set `KODOSI_PRODUCTION_DATA_ROOT` to your real data root so the runtime
refuses to overlap with production data. Use `KODOSI__RUNTIME__INITIAL_SHELL` to choose the shell
for new terminals.

On macOS, the app gives its host `<KODOSI_DATA_ROOT>/harness-home` as `HOME`. On other platforms,
terminals inherit your real `HOME`, so point `KODOSI__RUNTIME__INITIAL_SHELL` at a wrapper script
that sets a disposable `HOME`. On Linux, a host spawned by CLI commands inherits the systemd user
session environment (with your real `HOME`) and reads agent config directories for plan usage; run
it with a disposable `HOME` and a bogus `DBUS_SESSION_BUS_ADDRESS`. Never touch real provider
settings, memory, conversations, credentials, or working files.

The app and the host write redacted `app.log` and `host.log` files into the data root; inspect them
for sensitive data before attaching them to issues. Setting `KODOSI_TERMINAL_DEBUG=1` makes the
macOS and iOS apps log terminal view sizing and lifecycle events to stderr.

Agent chat adapters live in `runtime/links/` (and `kodosi acp` for Antigravity CLI). After
changing an adapter or upgrading an agent CLI, test the real agent inside an isolated host and
verify its chat view in the app. For Claude Code, run `claude plugin validate` against the generated
plugin in the data root. Chat replay fixtures come from real agent sessions; record fresh ones
with the agent and scrub private paths before committing.

## Data formats

See [AGENTS.md](../AGENTS.md) for when saved formats can change without backward compatibility.
Database changes always need a new migration (`dotnet ef migrations add <Name>` inside
`backend/`). Any breaking protocol change must bump the minimum supported version in `protocol/`
in the same change.
