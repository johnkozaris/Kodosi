# Development

[AGENTS.md](../AGENTS.md) lists the folders and [Architecture](ARCHITECTURE.md) shows how
they connect.

## Toolchains

Install `just`, Python 3 and the toolchains of the part you change.
[rust-toolchain.toml](../rust-toolchain.toml) pins Rust and
[backend/global.json](../backend/global.json) pins .NET; put that .NET SDK in
`backend/.tools/dotnet/`. `just rust-tools-install` puts the Rust check tools in
`.tools/rust/`. Keep every tool and dependency inside the project. The
[Mac](../clients/macos/README.md), [Linux and Windows](../clients/qt/README.md),
[iOS](../clients/ios/README.md) and [Android](../clients/android/README.md) guides list the
native prerequisites; on Windows run the recipes in Git Bash. The apps need Bun for the
document views.

## Commands

Run these from the repository root; `just --list` shows the rest.

| Command | Does |
| --- | --- |
| `just rust-cli-build` | Build the development CLI |
| `just agent-browser-build` | Build the pinned agent-browser of the shared browser next to the development CLI |
| `just documents-check` | Build and test the document views and crates |
| `just documents-app-check` | Lint the documents app, a separate build |
| `just documents-app-run <files>` | Run the documents app with a window for each file |
| `just browser-check` | Lint and test the shared browser crate and Kodosi Browser |
| `just browser-engine-build` | Build Kodosi Browser for development in `.tools/kodosi-browser/` |
| `just rust-check` | All Rust checks |
| `just backend-check` | Build and test the backend (database tests need Docker) |
| `just check-all` | The Rust, documents, browser, backend and translation checks |
| `just mac-build`, `just mac-check` | Build or check the Mac app |
| `just linux-build`, `just linux-check` | Build or check the Linux app |
| `just windows-check` | Build and check the Windows app, on Windows |
| `just windows-rust-lint` | Lint the runtime for Windows in Docker, from any host |
| `just ios-build`, `just ios-check` | Build or check the iPhone and iPad app |
| `just android-build`, `just android-check` | Build or check the Android app |
| `just sign-in-check` | Check and build the sign-in pages |
| `just translations-check` | Check that each text has its words in each language |

`gitleaks git`, run by hand, looks for secrets in the Git history and skips the reviewed
findings in [.gitleaksignore](../.gitleaksignore).

The justfile in each app's folder has its release recipes.

## Before a pull request

Run the check of each part that you changed:

| You changed | Run |
| --- | --- |
| The runtime, a contract or a Rust dependency | `just rust-check` |
| The documents or the shared browser | `just documents-check`, `just browser-check` |
| The backend | `just backend-check` |
| The sign-in pages | `just sign-in-check` |
| A text that people read | `just translations-check` |
| The Mac app, the iPhone and iPad app or their shared code | `just mac-check`, `just ios-check` |
| The Linux and Windows app | `just linux-check` on Linux or `just windows-check` on Windows |
| The Android app | `just android-check` |
| The scripts of the update site | `just release-test` |

A pull request runs the first five again, in a few minutes. No workflow builds an app for
a pull request. The Linux app, the Windows app and the Windows package build each night on
main. When you changed the Linux and Windows app and have no computer with Linux or
Windows, start the workflow `Linux app` or `Windows app` for your branch and wait for it:

```sh
gh workflow run "Linux app" --ref <branch>
```

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

Debug builds of the Mac, iPhone and Android apps use this local backend. For the Linux and
Windows app and the CLI, set `KODOSI__BACKEND__API=http://127.0.0.1:5180`, and
`KODOSI__AUTH__ISSUER` for another sign-in service.

## Contracts and Ghostty

`just protocol-gen` writes `protocol/desktop-runtime.json` from the Rust types, and the
Rust tests check it. After a change of the C API, copy the header that the
`kodosi-ffi-c` build writes to `clients/apple/Sources/KodosiKit/kodosi_runtime.h`.
`just ghostty-check` checks the Ghostty pins, and
[Ghostty provenance](../terminal/ghostty/PROVENANCE.md) explains updates.

[AgentBrowser.lock](../AgentBrowser.lock) pins the agent-browser that each desktop app
ships next to the `kodosi` program, by its version and Git commit. Each app build makes
it from that commit with its own `Cargo.lock`, in `.tools/agent-browser/`. To move the
pin, check the release and its tag first, change both lines and build each app.

Kodosi Browser builds on the ready-built CEF that its `cef` crate pins, which the build
downloads and checks, so nothing compiles Chromium. `browser/build/build-engine.py
--output-dir <folder>` builds it on each computer with `uv`, with its archive and its entry
in the feed. `scripts/generate-engine-feed.py <folder>` writes the feed of the archives of all
computers and signs it with the updates key from standard input; each update site carries
that feed. A development build of Kodosi uses the engine whose program
`KODOSI_BROWSER_ENGINE` names, and no feed.

## Texts and languages

Write each new text in English in the code, then give it its words in German, Greek,
Spanish, French, Italian, Japanese, Korean and Simplified Chinese.

- Swift: `Text("…")` and `String(localized: "…")`, with `bundle: .module` in the shared
  package. After a Mac or iPhone build, `just apple-strings-sync` adds the new texts to the
  String Catalogs.
- Qt: `qsTr()` in QML and `tr()` in C++, with `%n` for a count. `just linux-strings-sync` adds
  the new texts to `clients/qt/i18n/`.
- Android: `stringResource(R.string.…)` in Compose, with the words in each
  `clients/android/app/src/main/res/values*/strings.xml`.
- Runtime: `text!("… {name} …", name = value)` for a sentence for people, with its words in
  `runtime/texts/`. The detail of an internal fault stays English, as `Text::plain`.
- Backend: a refusal that people read stays an English sentence in the backend. List it in
  `backend_refusals()` in `runtime/src/network/http.rs`, with its words in `runtime/texts/`.
- Document views: `say("…")` from `@documents/words`, with its words in
  `documents/views/words/src/`.
- Sign-in theme: `sign-in/src/login/i18n.ts`, `sign-in/src/account/words.ts` and
  `sign-in/src/email/messages/`.

`just translations-check` lists each text without words in a language, and each
translation that lost or changed a value. `just mac-check`, `just ios-check` and
`just linux-check` also fail when a String Catalog or a Qt translation file misses a text
of the code.

## Isolated validation

Validate by running the app or calling the real boundary, with a disposable database and
a disposable `KODOSI_DATA_ROOT`. Set `KODOSI_PRODUCTION_DATA_ROOT` to the real data root
too, so the runtime refuses an overlap. `KODOSI__RUNTIME__INITIAL_SHELL` picks the shell
of new terminals. The Mac app gives its host `<KODOSI_DATA_ROOT>/harness-home` as `HOME`;
elsewhere terminals get your real `HOME`, so point the shell variable at a script that
sets a disposable one. On Linux, a host that a command starts takes the environment of
the systemd user session, with your real `HOME`, and reads the agents' own folders for
plan use; run it with a disposable `HOME` and a `DBUS_SESSION_BUS_ADDRESS` that points
nowhere. Never touch real provider settings, memory, conversations,
credentials or working files. The app writes a redacted `app.log` in the data root, and
the host `host.log`; check them for private data before you attach them to an issue.
With `KODOSI_TERMINAL_DEBUG=1`, the Mac and iPhone apps write the sizes and lifecycle of
their terminal views to stderr.

## Data formats

[AGENTS.md](../AGENTS.md) says when a format can change without a reader for the old one.
A change of the database is a new migration (`dotnet ef migrations add <Name>` in
`backend/`). A contract change that older apps cannot use raises the oldest version in
`protocol/` in the same change.
