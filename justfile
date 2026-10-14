set shell := ["bash", "-euo", "pipefail", "-c"]

cargo_tools_root := justfile_directory() + "/.tools/rust"
cargo_tools_bin := cargo_tools_root + "/bin"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/terminal/ghostty")

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
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-deny --version 0.19.0
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-machete --version 0.9.1
    cargo install --locked --root "{{ cargo_tools_root }}" cargo-llvm-cov --version 0.8.5

# Install Bacon for the jobs in bacon.toml, and the Rust check tools that the jobs use.
[group('tools')]
bacon-install: rust-tools-install
    cargo install --locked --root "{{ cargo_tools_root }}" bacon --version 3.22.0

[private]
rust-dependency-tools-check:
    @test -x "{{ cargo_tools_bin }}/cargo-deny" -a -x "{{ cargo_tools_bin }}/cargo-machete" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

[private]
rust-coverage-tools-check:
    @test -x "{{ cargo_tools_bin }}/cargo-llvm-cov" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

# Do the Ghostty pin, artifact and notice checks without a build.
[group('ghostty')]
ghostty-check: ghostty-host-check
    python3 terminal/ghostty/scripts/verify-ghostty-pins.py

[private]
[macos]
ghostty-host-check:
    python3 "{{ ghostty_dir }}/scripts/tests/test_digest.py"
    "{{ ghostty_dir }}/scripts/verify-third-party-notices.py"

[private]
[linux]
ghostty-host-check:
    python3 "{{ ghostty_dir }}/scripts/tests/test_digest.py"
    "{{ ghostty_dir }}/scripts/verify-linux-third-party-notices.py"

# Git Bash has no ar, lipo or plutil for the notice checks.
[private]
[windows]
ghostty-host-check:
    python3 "{{ ghostty_dir }}/scripts/tests/test_digest.py"

# Build the development CLI in runtime/target/debug.
[group('rust')]
rust-cli-build: conpty-install
    cargo build --manifest-path runtime/Cargo.toml --locked --bin kodosi

# Compile the Rust workspace and each runtime feature set, and stop on a warning.
[group('rust')]
rust-compile-check: rust-features-check
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked --workspace

[private]
rust-features-check:
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --features host --lib
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib
    CARGO_BUILD_WARNINGS=deny cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib --no-run
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-ffi-c
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-ffi-c --no-default-features
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --bins -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used

# Make sure that the Rust code has the standard format. Change no file.
[group('rust')]
rust-format-check:
    cargo fmt --manifest-path runtime/Cargo.toml --all -- --check

# Build the Rust documentation and stop on a warning.
[group('rust')]
rust-doc-build:
    CARGO_BUILD_WARNINGS=deny RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path runtime/Cargo.toml --locked --workspace --no-deps

# Lint the Rust workspace with Clippy.
[group('rust')]
rust-lint:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --lib --bins --examples -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used

# Examine the Rust dependencies for advisories, bans, licenses and sources with cargo-deny.
[group('rust')]
rust-deny-check: rust-dependency-tools-check
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --config deny.toml --hide-inclusion-graph advisories bans licenses sources

# Find the Rust dependencies that the code does not use, with cargo-machete.
[group('rust')]
rust-machete-check: rust-dependency-tools-check
    cargo machete --skip-target-dir runtime

# Run rust-deny-check and rust-machete-check.
[group('rust')]
rust-dependency-check: rust-deny-check rust-machete-check

# Windows terminals need the pinned ConPTY host beside the development CLI, and
# tests find it through KODOSI_CONPTY_DIR.
[private]
[windows]
conpty-install:
    python3 terminal/conpty/install-conpty.py --arch "$(source scripts/windows-env.sh && kodosi_windows_arch)" --output "$KODOSI_CONPTY_DIR"

[private]
[unix]
conpty-install:

# Run the Rust tests for the full and the viewer builds, and the documentation tests.
[group('rust')]
rust-test: conpty-install
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --doc

# Write an HTML coverage report to runtime/target/llvm-cov.
[group('rust')]
rust-coverage-gen: rust-coverage-tools-check
    cargo llvm-cov --manifest-path runtime/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --html --output-dir runtime/target/llvm-cov

# Write an LCOV coverage report to runtime/target/llvm-cov/lcov.info.
[group('rust')]
rust-lcov-gen: rust-coverage-tools-check
    mkdir -p runtime/target/llvm-cov
    cargo llvm-cov --manifest-path runtime/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --lcov --output-path runtime/target/llvm-cov/lcov.info

# Stop if the line coverage of the Rust tests is less than the minimum.
[group('rust')]
rust-coverage-check: rust-coverage-tools-check
    mkdir -p runtime/target/llvm-cov
    cargo llvm-cov --manifest-path runtime/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --fail-under-lines 55 --json --summary-only --output-path runtime/target/llvm-cov/summary.json

# Write the generated contracts and the C header from the Rust definitions.
[group('protocol')]
protocol-gen:
    cargo run --manifest-path runtime/Cargo.toml --locked --bin protocol-gen -- write

# Make sure that the committed contracts agree with the Rust definitions.
[group('protocol')]
protocol-check:
    cargo run --manifest-path runtime/Cargo.toml --locked --bin protocol-gen -- check
    cargo run --manifest-path runtime/Cargo.toml --locked --no-default-features --bin protocol-gen -- check

# Run the Ghostty checks and all the Rust and contract checks.
[group('rust')]
rust-check: ghostty-check rust-format-check rust-compile-check rust-doc-build rust-lint rust-dependency-check protocol-check rust-test rust-coverage-check

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

# Build the backend release image for a platform, for example linux/arm64, and give it a tag. Push nothing.
[group('backend')]
backend-package platform tag="kodosi-backend":
    docker buildx build --platform "{{ platform }}" --file backend/Dockerfile --tag "{{ tag }}" --load .

[private]
backend-sdk-check:
    python3 backend/scripts/verify-sdk-pin.py

# Run all the backend checks.
[group('backend')]
backend-check: backend-sdk-check backend-test

# Run the Rust and backend checks.
check-all: rust-check backend-check

# Generate the Xcode project, test the shared Swift package and build the Debug Mac app.
[group('mac')]
mac-build:
    just --justfile clients/macos/justfile build

# Lint and test the Mac app and its scripts, then build the Release app. Commit all the changes first.
[group('mac')]
mac-check:
    just --justfile clients/macos/justfile check

# Build the iPhone and iPad app for the iOS simulator.
[group('ios')]
ios-build:
    just --justfile clients/ios/justfile build

# Lint and test the iPhone and iPad app, then build it for devices.
[group('ios')]
ios-check:
    just --justfile clients/ios/justfile check

# Install the Rust targets for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-targets-install:
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Lint the viewer runtime for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-lint:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --target aarch64-apple-ios --target aarch64-apple-ios-sim -- -D warnings

# Build the Linux app.
[group('linux')]
linux-build:
    just --justfile clients/qt/justfile build

# Lint and test the Linux app and its release and license scripts.
[group('linux')]
linux-check:
    just --justfile clients/qt/justfile check

# Build and test the Windows app on Windows in Git Bash.
[group('windows')]
windows-check:
    just --justfile clients/qt/justfile windows-check

# Lint the runtime for the two Windows targets in Docker on any host.
[group('windows')]
windows-rust-lint packages="--workspace":
    docker build --quiet --tag kodosi-windows-cross --file scripts/windows-cross.Dockerfile scripts >/dev/null
    for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do \
        docker run --rm --volume "$PWD:/src" --volume kodosi-cargo-registry:/usr/local/cargo/registry --volume kodosi-xwin-cache:/cache/xwin --workdir /src --env CARGO_BUILD_WARNINGS=deny --env CARGO_INCREMENTAL=0 --env KODOSI_GHOSTTY_CHECK_ONLY=1 kodosi-windows-cross bash -euo pipefail -c "\
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target {{ packages }} --lib --bins --examples -- -D warnings && \
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target {{ packages }} --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used"; \
    done

# Build the Keycloak theme after the type, lint and format checks.
[group('sign-in')]
sign-in-check:
    cd sign-in && bun install --frozen-lockfile && bun run tsc && bun run lint && bun run format:check && bun run build

# Test the script that writes the update site of a release.
[group('release')]
release-test:
    python3 -m unittest scripts/tests/test_update_site.py
