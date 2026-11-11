set shell := ["bash", "-euo", "pipefail", "-c"]

cargo_tools_root := justfile_directory() + "/.tools/rust"
cargo_tools_bin := cargo_tools_root + "/bin"
cargo_deny_version := "0.20.2"
cargo_machete_version := "0.9.2"
cargo_ndk_version := "4.1.2"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/terminal/ghostty")
test_lints := "-D warnings -A clippy::unwrap_used -A clippy::expect_used"

path_separator := if os_family() == "windows" { ";" } else { ":" }

export PATH := justfile_directory() + "/backend/.tools/dotnet" + path_separator + cargo_tools_bin + path_separator + env('PATH')
export KODOSI_GHOSTTY_DIR := ghostty_dir
export KODOSI_CONPTY_DIR := env('KODOSI_CONPTY_DIR', justfile_directory() + "/runtime/target/debug")

[private]
default:
    @just --list

# Install the Rust check tools in .tools/rust.
[group('tools')]
rust-tools-install:
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-deny --version {{ cargo_deny_version }}
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-machete --version {{ cargo_machete_version }}

[private]
rust-tool-check tool:
    @test -x "{{ cargo_tools_bin }}/{{ tool }}" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

# Check the Ghostty pins, digests and notices without a build.
[group('ghostty')]
ghostty-check: ghostty-notices-check
    python3 terminal/ghostty/scripts/verify-ghostty-pins.py
    python3 "{{ ghostty_dir }}/scripts/tests/test_digest.py"

[private]
[macos]
ghostty-notices-check:
    "{{ ghostty_dir }}/scripts/verify-third-party-notices.py"
    "{{ ghostty_dir }}/scripts/verify-vt-third-party-notices.py" --layout android-arm64

[private]
[linux]
ghostty-notices-check:
    "{{ ghostty_dir }}/scripts/verify-vt-third-party-notices.py" --layout linux-x86_64
    "{{ ghostty_dir }}/scripts/verify-vt-third-party-notices.py" --layout android-arm64

# Git Bash has no ar or nm for the notice checks.
[private]
[windows]
ghostty-notices-check:

# Build the development CLI in runtime/target/debug.
[group('rust')]
rust-cli-build: conpty-install
    cargo build --manifest-path runtime/Cargo.toml --locked --bin kodosi

# Build Kodosi Browser for development into .tools/kodosi-browser; KODOSI_BROWSER_ENGINE names it.
[group('rust')]
browser-engine-build:
    python3 browser/build/build-engine.py --debug --output-dir .tools/kodosi-browser

# Build the pinned agent-browser of the shared browser next to the development CLI.
[group('rust')]
agent-browser-build:
    python3 scripts/build-agent-browser.py --target "$(rustc -vV | sed -n 's/^host: //p')" --output-dir runtime/target/debug

# Check the Rust format.
[group('rust')]
rust-format-check:
    cargo fmt --manifest-path runtime/Cargo.toml --all -- --check

# Lint the full and the viewer builds of the Rust workspace with Clippy.
[group('rust')]
rust-lint:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --lib --bins --examples -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --tests -- {{ test_lints }}
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --bins -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --tests -- {{ test_lints }}

# Check the licenses and the sources of the Rust dependencies with cargo-deny.
[group('rust')]
rust-deny-check: (rust-tool-check "cargo-deny")
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --hide-inclusion-graph bans licenses sources

# Check the Rust dependencies for security advisories with cargo-deny.
[group('rust')]
rust-advisories-check: (rust-tool-check "cargo-deny")
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --hide-inclusion-graph advisories

# Find unused Rust dependencies with cargo-machete.
[group('rust')]
rust-machete-check: (rust-tool-check "cargo-machete")
    cargo machete --skip-target-dir runtime

# Run rust-deny-check and rust-machete-check.
[group('rust')]
rust-dependency-check: rust-deny-check rust-machete-check

# Windows terminals and tests find the pinned ConPTY host in KODOSI_CONPTY_DIR.
[private]
[windows]
conpty-install:
    python3 terminal/conpty/install-conpty.py --arch "$(source scripts/windows-env.sh && kodosi_windows_arch)" --output "$KODOSI_CONPTY_DIR"

[private]
[unix]
conpty-install:

# Run the Rust tests for the full and the viewer builds.
[group('rust')]
rust-test: conpty-install
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --all-targets

# Write protocol/desktop-runtime.json from the Rust definitions. The tests check it.
[group('protocol')]
protocol-gen:
    cargo run --manifest-path runtime/Cargo.toml --locked --bin protocol-gen

# Run the Ghostty checks and all the Rust checks.
[group('rust')]
rust-check: ghostty-check rust-format-check rust-lint rust-dependency-check rust-test

# Build, check and test the views, then lint and test the document crates.
[group('rust')]
documents-check:
    cd documents/views && bun install --frozen-lockfile && bun run build && bun run format:check && bun run test
    cargo fmt --manifest-path documents/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path documents/Cargo.toml --locked --workspace --all-targets -- {{ test_lints }}
    cd documents && cargo test --locked --workspace

# Build the views, then lint the documents app, which has its own workspace and Cargo.lock.
[group('rust')]
documents-app-check:
    cd documents/views && bun install --frozen-lockfile && bun run build
    cargo fmt --manifest-path documents/app/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path documents/app/Cargo.toml --locked --all-targets -- {{ test_lints }}

# Run the documents app with a window for each file; one file twice gives two people.
[group('rust')]
[no-cd]
[positional-arguments]
documents-app-run *files:
    cd "{{ justfile_directory() }}/documents/views" && bun install --frozen-lockfile && bun run build
    cargo run --manifest-path "{{ justfile_directory() }}/documents/app/Cargo.toml" --locked -- "$@"

# Lint and test the shared browser crate and Kodosi Browser.
[group('rust')]
browser-check:
    cargo fmt --manifest-path browser/crate/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path browser/crate/Cargo.toml --locked --all-targets -- {{ test_lints }}
    cargo test --manifest-path browser/crate/Cargo.toml --locked
    python3 browser/build/build-engine.py --check

# Install the backend packages in locked mode.
[group('backend')]
backend-packages-install:
    cd backend && dotnet restore Kodosi.slnx --locked-mode

# Build the backend.
[group('backend')]
backend-build: backend-packages-install
    cd backend && dotnet build Kodosi.slnx --no-restore

# Build the backend and run its tests. The database tests need a container runtime.
[group('backend')]
backend-test: backend-build
    cd backend && dotnet test --solution Kodosi.slnx --no-restore --no-build

# Apply the database migrations. Make a backup of the data first.
[group('backend')]
backend-migrate: backend-packages-install
    cd backend && dotnet run --no-restore --project src/Kodosi.Host -- migrate

# Apply the database migrations, then start the backend.
[group('backend')]
backend-run: backend-migrate
    cd backend && dotnet run --no-restore --project src/Kodosi.Host

# Start PostgreSQL, the migrations and the backend in Docker Compose.
[group('backend')]
backend-docker-run:
    docker compose up --build --detach --wait

# Stop the Docker Compose services and keep the database volume.
[group('backend')]
backend-docker-stop:
    docker compose down

# Build and tag the backend release image for a platform, for example linux/arm64. Push nothing.
[group('backend')]
backend-package platform tag="kodosi-backend":
    docker buildx build --platform "{{ platform }}" --file backend/Dockerfile --tag "{{ tag }}" --load .

[private]
backend-sdk-check:
    python3 backend/scripts/verify-sdk-pin.py

# Run all the backend checks.
[group('backend')]
backend-check: backend-sdk-check backend-test

# Run the Rust, documents, browser, backend and translation checks.
check-all: rust-check documents-check browser-check backend-check translations-check

# Check that each text that people read has its words in each language.
[group('translations')]
translations-check:
    python3 scripts/check-translations.py

# Add the texts of the last Mac and iPhone builds to the string catalogs.
[group('translations')]
apple-strings-sync:
    python3 clients/apple/scripts/sync-string-catalogs.py

# Generate the Xcode project, test the shared Swift package and build the Debug Mac app.
[group('mac')]
mac-build:
    just --justfile clients/macos/justfile build

# Lint and test the Mac app, build the Release app, then check its translations. Commit all the changes first.
[group('mac')]
mac-check:
    just --justfile clients/macos/justfile check

# Build the iPhone and iPad app for the iOS simulator.
[group('ios')]
ios-build:
    just --justfile clients/ios/justfile build

# Lint the runtime for iOS, lint and test the iPhone and iPad app, build it for devices, then check its translations.
[group('ios')]
ios-check: ios-rust-lint
    just --justfile clients/ios/justfile check

# Install the Rust targets for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-targets-install:
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Lint the viewer runtime for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-lint:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --target aarch64-apple-ios --target aarch64-apple-ios-sim -- -D warnings

# Install the Rust target for Android devices and cargo-ndk in .tools/rust.
[group('android')]
android-tools-install:
    rustup target add aarch64-linux-android
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-ndk --version {{ cargo_ndk_version }}

# Build the viewer runtime for Android as a shared library, for the dev or release profile.
[group('android')]
android-runtime-build profile="dev":
    just --justfile clients/android/justfile runtime-build {{ profile }}

# Lint the viewer runtime for Android devices.
[group('android')]
android-rust-lint:
    just --justfile clients/android/justfile rust-lint

# Build the Debug Android app.
[group('android')]
android-build:
    just --justfile clients/android/justfile build

# Build the optimized release APK, signed when the signing environment is set.
[group('android')]
android-release-build:
    just --justfile clients/android/justfile release-build

# Build the Android App Bundle with the same signing environment.
[group('android')]
android-bundle-build:
    just --justfile clients/android/justfile bundle-build

# Collect the Android viewer's dependency licenses.
[group('android')]
android-notices-build:
    just --justfile clients/android/justfile notices-build

# Install the Debug Android app on the device or emulator that adb sees, and open it.
[group('android')]
android-run:
    just --justfile clients/android/justfile run

# Check the Kotlin style and run Android lint on the Android app.
[group('android')]
android-lint:
    just --justfile clients/android/justfile lint

# Format the Kotlin sources of the Android app.
[group('android')]
android-format:
    just --justfile clients/android/justfile format

# Run the unit tests of the Android app.
[group('android')]
android-test:
    just --justfile clients/android/justfile test

# Test the JNI terminal and credential storage on an attached ARM64 Android device.
[group('android')]
android-device-test:
    just --justfile clients/android/justfile device-test

# Build the native Android boundary tests.
[group('android')]
android-device-test-build:
    just --justfile clients/android/justfile device-test-build

# Lint the runtime for Android, lint and test the Android app, build it, then check its translations.
[group('android')]
android-check: android-rust-lint
    just --justfile clients/android/justfile check

# Build the Linux app.
[group('linux')]
linux-build:
    just --justfile clients/qt/justfile build

# Lint and test the Linux app and check its dependency pins and license evidence.
[group('linux')]
linux-check:
    just --justfile clients/qt/justfile check

# Add the new texts of the Linux and Windows app to its translation files.
[group('linux')]
linux-strings-sync:
    just --justfile clients/qt/justfile strings-sync

# Build and test the Windows app on Windows in Git Bash.
[group('windows')]
windows-check:
    just --justfile clients/qt/justfile windows-check

# Lint the runtime for the two Windows targets in Docker on any host.
[group('windows')]
windows-rust-lint:
    docker build --quiet --build-arg "RUST_VERSION=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)" \
        --tag kodosi-windows-cross --file scripts/windows-cross.Dockerfile scripts >/dev/null
    for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do \
        MSYS_NO_PATHCONV=1 docker run --rm --volume "$PWD:/src" --volume kodosi-cargo-registry:/usr/local/cargo/registry --volume kodosi-xwin-cache:/cache/xwin --workdir /src --env CARGO_BUILD_WARNINGS=deny --env CARGO_INCREMENTAL=0 --env KODOSI_GHOSTTY_CHECK_ONLY=1 kodosi-windows-cross bash -euo pipefail -c "\
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target --workspace --lib --bins --examples -- -D warnings && \
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target --workspace --tests -- {{ test_lints }}"; \
    done

# Build the Keycloak theme after the type, lint and format checks.
[group('sign-in')]
sign-in-check: translations-check
    cd sign-in && bun install --frozen-lockfile && bun run tsc && bun run lint && bun run format:check && bun run build

# Test the scripts that write the update site of a release and the feed of Kodosi Browser.
[group('release')]
release-test:
    python3 -m unittest scripts/tests/test_update_site.py scripts/tests/test_engine_feed.py
