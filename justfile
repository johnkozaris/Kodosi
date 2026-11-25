set shell := ["bash", "-euo", "pipefail", "-c"]

cargo_tools_root := justfile_directory() + "/.tools/rust"
cargo_tools_bin := cargo_tools_root + "/bin"
cargo_deny_version := "0.22.0"
cargo_machete_version := "0.10.1"
cargo_ndk_version := "4.1.2"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/terminal/ghostty")
test_lints := "-D warnings -A clippy::unwrap_used -A clippy::expect_used"

path_separator := if os_family() == "windows" { ";" } else { ":" }
bun_bin := justfile_directory() + "/.tools/bun/" + trim(read(".bun-version"))

export PATH := bun_bin + path_separator + justfile_directory() + "/backend/.tools/dotnet" + path_separator + cargo_tools_bin + path_separator + env('PATH')
export KODOSI_GHOSTTY_DIR := ghostty_dir
export KODOSI_CONPTY_DIR := env('KODOSI_CONPTY_DIR', justfile_directory() + "/runtime/target/debug")

[private]
default:
    @just --list

# Install the Rust check tools in .tools/rust.
[group('tools')]
rust-tools-install:
    cargo install --root "{{ cargo_tools_root }}" cargo-deny --version {{ cargo_deny_version }}
    cargo install --root "{{ cargo_tools_root }}" cargo-machete --version {{ cargo_machete_version }}

[private]
bun-install:
    @python3 scripts/bun.py > /dev/null

[private]
rust-tool-check tool:
    @test -x "{{ cargo_tools_bin }}/{{ tool }}" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

# Check Ghostty pins, digests, and notices without building.
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

# Build Kodosi Browser for development into .tools/kodosi-browser; point KODOSI_BROWSER_ENGINE to it.
[group('rust')]
browser-engine-build:
    python3 browser/build/build-engine.py --debug --output-dir .tools/kodosi-browser

# Build the pinned agent-browser binary next to the development CLI.
[group('rust')]
agent-browser-build:
    python3 scripts/build-agent-browser.py --target "$(rustc -vV | sed -n 's/^host: //p')" --output-dir runtime/target/debug

# Check Rust formatting.
[group('rust')]
rust-format-check:
    cargo fmt --manifest-path runtime/Cargo.toml --all -- --check

# Lint both the full and viewer Rust workspace builds with Clippy.
[group('rust')]
rust-lint:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --lib --bins --examples -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --tests -- {{ test_lints }}
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --bins -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --tests -- {{ test_lints }}

# Check Rust dependency licenses and sources with cargo-deny.
[group('rust')]
rust-deny-check: (rust-tool-check "cargo-deny")
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --hide-inclusion-graph bans licenses sources

# Check app and Kodosi Browser Rust dependencies for security advisories with cargo-deny.
[group('rust')]
rust-advisories-check: (rust-tool-check "cargo-deny")
    cargo deny --manifest-path browser/engine/Cargo.toml --locked check --hide-inclusion-graph advisories
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --hide-inclusion-graph advisories

# Check document-view and sign-in Bun packages for security advisories.
[group('rust')]
[group('sign-in')]
bun-advisories-check: bun-install
    failed=0; \
    (cd documents/views && bun audit) || failed=1; \
    (cd sign-in && bun audit) || failed=1; \
    exit "$failed"

# Find unused Rust dependencies with cargo-machete.
[group('rust')]
rust-machete-check: (rust-tool-check "cargo-machete")
    cargo machete --skip-target-dir runtime

# Check that all Linux and Windows runtime Rust dependencies have license text.
[group('rust')]
rust-license-check:
    for target in x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do \
        python3 clients/qt/scripts/release/generate-rust-license-inventory.py \
            --manifest runtime/Cargo.toml \
            --target "$target" \
            --output-dir "runtime/target/rust-license-check/$target"; \
    done

# Run rust-deny-check, rust-machete-check and rust-license-check.
[group('rust')]
rust-dependency-check: rust-deny-check rust-machete-check rust-license-check

# Windows terminals and tests find the pinned ConPTY host in KODOSI_CONPTY_DIR.
[private]
[windows]
conpty-install:
    python3 terminal/conpty/install-conpty.py --arch "$(source scripts/windows-env.sh && kodosi_windows_arch)" --output "$KODOSI_CONPTY_DIR"

[private]
[unix]
conpty-install:

# Run Rust tests for both the full and viewer builds.
[group('rust')]
rust-test: conpty-install
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --all-targets

# Generate protocol/desktop-runtime.json from Rust definitions (verified by tests).
[group('protocol')]
protocol-gen:
    cargo run --manifest-path runtime/Cargo.toml --locked --bin protocol-gen

# Run Ghostty checks and all Rust checks.
[group('rust')]
rust-check: ghostty-check rust-format-check rust-lint rust-dependency-check rust-test

# Build, check and test the views, then lint and test the document crates.
[group('rust')]
documents-check: bun-install
    cd documents/views && bun ci && bun run build && bun run format:check && bun run test
    cargo fmt --manifest-path documents/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path documents/Cargo.toml --locked --workspace --all-targets -- {{ test_lints }}
    cd documents && cargo test --locked --workspace

# Build the views, then lint the documents app, which has its own workspace and Cargo.lock.
[group('rust')]
documents-app-check: bun-install
    cd documents/views && bun ci && bun run build
    cargo fmt --manifest-path documents/app/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path documents/app/Cargo.toml --locked --all-targets -- {{ test_lints }}

# Run the documents app with one window per file; pass a file twice to simulate two users.
[group('rust')]
[no-cd]
[positional-arguments]
documents-app-run *files: bun-install
    cd "{{ justfile_directory() }}/documents/views" && bun ci && bun run build
    cargo run --manifest-path "{{ justfile_directory() }}/documents/app/Cargo.toml" --locked -- "$@"

# Lint and test the shared browser crate and Kodosi Browser.
[group('rust')]
browser-check:
    cargo fmt --manifest-path browser/crate/Cargo.toml --all -- --check
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path browser/crate/Cargo.toml --locked --all-targets -- {{ test_lints }}
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path browser/crate/Cargo.toml --locked --no-default-features --all-targets -- {{ test_lints }}
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

# Build and tag the backend release image for a platform (e.g. linux/arm64) without pushing.
[group('backend')]
backend-package platform tag="kodosi-backend":
    docker buildx build --platform "{{ platform }}" --file backend/Dockerfile --tag "{{ tag }}" --load .

[private]
backend-sdk-check:
    python3 backend/scripts/verify-sdk-pin.py

# Run all backend checks.
[group('backend')]
backend-check: backend-sdk-check backend-test

# Run the Rust, documents, browser, backend and translation checks.
check-all: rust-check documents-check browser-check backend-check translations-check

# Check that every user-facing string is translated into all supported languages.
[group('translations')]
translations-check:
    python3 scripts/check-translations.py

# Sync strings from the latest macOS and iOS builds into the string catalogs.
[group('translations')]
apple-strings-sync:
    python3 clients/apple/scripts/sync-string-catalogs.py

# Generate the Xcode project, test the shared Swift package and build the Debug Mac app.
[group('mac')]
mac-build:
    just --justfile clients/macos/justfile build

# Lint and test the Mac app, build Release, and check translations (commit changes first).
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
    cargo install --root "{{ cargo_tools_root }}" cargo-ndk --version {{ cargo_ndk_version }}

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

# Install and launch the Debug Android app on the connected adb device or emulator.
[group('android')]
android-run:
    just --justfile clients/android/justfile run

# Format Android Kotlin sources.
[group('android')]
android-format:
    just --justfile clients/android/justfile format

# Run Android unit tests.
[group('android')]
android-test:
    just --justfile clients/android/justfile test

# Test the JNI terminal and credential storage on an attached ARM64 Android device.
[group('android')]
android-device-test:
    just --justfile clients/android/justfile device-test

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

# Sync new Linux and Windows app strings into the translation files.
[group('linux')]
linux-strings-sync:
    just --justfile clients/qt/justfile strings-sync

# Build and test the Windows app on Windows in Git Bash.
[group('windows')]
windows-check:
    just --justfile clients/qt/justfile windows-check

# Lint the runtime for both Windows targets in Docker on any host.
[group('windows')]
windows-rust-lint:
    docker build --quiet --build-arg "RUST_VERSION=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)" \
        --tag kodosi-windows-cross --file scripts/windows-cross.Dockerfile scripts >/dev/null
    for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do \
        MSYS_NO_PATHCONV=1 docker run --rm --volume "$PWD:/src" --volume kodosi-cargo-registry:/usr/local/cargo/registry --volume kodosi-xwin-cache:/cache/xwin --workdir /src --env CARGO_BUILD_WARNINGS=deny --env CARGO_INCREMENTAL=0 --env KODOSI_GHOSTTY_CHECK_ONLY=1 kodosi-windows-cross bash -euo pipefail -c "\
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target --workspace --lib --bins --examples -- -D warnings && \
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target --workspace --tests -- {{ test_lints }}"; \
    done

# Type-check, lint, format-check, and build the Keycloak theme, then build and test the extension.
[group('sign-in')]
sign-in-check: bun-install translations-check
    cd sign-in && bun ci && bun run lint && bun run format:check && bun run build
    sign-in/extension/maven.sh -q verify

# Test the release update-site and Kodosi Browser feed scripts.
[group('release')]
release-test:
    python3 -m unittest scripts/tests/test_update_site.py scripts/tests/test_engine_feed.py
