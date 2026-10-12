set shell := ["bash", "-euo", "pipefail", "-c"]

cargo_tools_root := justfile_directory() + "/.tools/rust"
cargo_tools_bin := cargo_tools_root + "/bin"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/terminal/ghostty")

path_separator := if os_family() == "windows" { ";" } else { ":" }

export PATH := justfile_directory() + "/backend/.tools/dotnet-10.0.401" + path_separator + cargo_tools_bin + path_separator + env('PATH')
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

# Install the Rust check tools, and Bacon for the jobs in bacon.toml.
[group('tools')]
dev-tools-install: rust-tools-install
    cargo install --locked --root "{{ cargo_tools_root }}" bacon --version 3.22.0

_dependency-tools:
    @test -x "{{ cargo_tools_bin }}/cargo-deny" -a -x "{{ cargo_tools_bin }}/cargo-machete" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

_coverage-tool:
    @test -x "{{ cargo_tools_bin }}/cargo-llvm-cov" || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

# Do the Ghostty pin and artifact checks without a build.
[group('ghostty')]
ghostty-verify:
    scripts/verify-ghostty.sh

# Build the development CLI in runtime/target/debug.
[group('rust')]
rust-cli-build: conpty
    cargo build --manifest-path runtime/Cargo.toml --locked --bin kodosi

# Compile the Rust workspace and each runtime feature set.
[group('rust')]
rust-check: rust-feature-gates
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked --workspace

[private]
rust-feature-gates:
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --features host --lib
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib
    CARGO_BUILD_WARNINGS=deny cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib --no-run
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-ffi-c
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path runtime/Cargo.toml --locked -p kodosi-ffi-c --no-default-features
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --bins -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used

# Do a format check of the Rust code.
[group('rust')]
rust-fmt:
    cargo fmt --manifest-path runtime/Cargo.toml --all -- --check

# Build the Rust documentation and stop on a warning.
[group('rust')]
rust-doc:
    CARGO_BUILD_WARNINGS=deny RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path runtime/Cargo.toml --locked --workspace --no-deps

# Lint the Rust workspace with Clippy.
[group('rust')]
rust-clippy:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --lib --bins --examples -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked --workspace --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used

# Examine the Rust dependencies for advisories, bans, licenses and sources.
[group('rust')]
rust-deny: _dependency-tools
    cargo deny --manifest-path runtime/Cargo.toml --workspace --locked check --config deny.toml --hide-inclusion-graph advisories bans licenses sources

# Find the Rust dependencies that the code does not use.
[group('rust')]
rust-machete: _dependency-tools
    cargo machete --skip-target-dir runtime

# Run rust-deny and rust-machete.
[group('rust')]
rust-dependency-hygiene: rust-deny rust-machete

[private]
ffi-publication-test:
    PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-ffi-artifact.py

# Windows terminals need the pinned ConPTY host beside the development CLI, and
# tests find it through KODOSI_CONPTY_DIR.
[private]
[windows]
conpty:
    python3 terminal/conpty/fetch.py --arch "$(source scripts/windows-env.sh && kodosi_windows_arch)" --output "$KODOSI_CONPTY_DIR"

[private]
[unix]
conpty:

# Run the Rust tests for the full and the viewer builds, and the documentation tests.
[group('rust')]
rust-test: ffi-publication-test conpty
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path runtime/Cargo.toml --locked --workspace --doc

# Write an HTML coverage report to runtime/target/llvm-cov.
[group('rust')]
rust-coverage: _coverage-tool
    cargo llvm-cov --manifest-path runtime/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --html --output-dir runtime/target/llvm-cov

# Write an LCOV coverage report to runtime/target/llvm-cov/lcov.info.
[group('rust')]
rust-coverage-lcov: _coverage-tool
    mkdir -p runtime/target/llvm-cov
    cargo llvm-cov --manifest-path runtime/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --lcov --output-path runtime/target/llvm-cov/lcov.info

# Stop if the line coverage of the Rust tests is less than the minimum.
[group('rust')]
rust-coverage-check: _coverage-tool
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
rust-all: ghostty-verify rust-fmt rust-check rust-doc rust-clippy rust-dependency-hygiene protocol-check rust-test rust-coverage-check

# Restore the backend packages in locked mode.
[group('backend')]
backend-restore:
    cd backend && dotnet restore Kodosi.slnx --locked-mode

# Build the backend.
[group('backend')]
backend-build: backend-restore
    cd backend && dotnet build Kodosi.slnx --no-restore

# Build the backend and run its tests. The database tests need a container runtime.
[group('backend')]
backend-test: backend-build
    cd backend && dotnet test --solution Kodosi.slnx --no-restore --no-build

# Apply the database migrations. Make a backup of the data first.
[group('backend')]
backend-migrate: backend-restore
    cd backend && dotnet run --no-restore --project src/Kodosi.Host -- migrate

# Apply the database migrations, then start the backend.
[group('backend')]
backend-run: backend-migrate
    cd backend && dotnet run --no-restore --project src/Kodosi.Host

# Start PostgreSQL, the migrations and the backend in Docker Compose.
[group('backend')]
backend-docker-up:
    docker compose up --build --detach --wait

# Stop the Docker Compose services and keep the database volume.
[group('backend')]
backend-docker-down:
    docker compose down

# Run all the backend checks.
[group('backend')]
backend-all: backend-test

# Run the Rust and backend checks.
check-all: rust-all backend-all

# Make the Xcode project, run the package tests and build the Debug Mac app.
[group('mac')]
mac-build:
    just --justfile clients/macos/Justfile build-debug

# Lint and test the Mac app and its build tools, then build the Release app.
[group('mac')]
mac-check:
    just --justfile clients/macos/Justfile ci

# Build the iPhone and iPad app for the iOS simulator.
[group('ios')]
ios-build:
    just --justfile clients/ios/Justfile build

# Lint and test the iPhone and iPad app, then build it for devices.
[group('ios')]
ios-check:
    just --justfile clients/ios/Justfile ci

# Install the Rust targets for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-targets:
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Lint the viewer runtime for iOS devices and the iOS simulator.
[group('ios')]
ios-rust-check:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path runtime/Cargo.toml --locked -p kodosi-runtime -p kodosi-ffi-c --no-default-features --lib --target aarch64-apple-ios --target aarch64-apple-ios-sim -- -D warnings

# Build the iOS runtime library for an SDK and a profile, and install it for Xcode.
[group('ios')]
ios-ffi-build sdk profile:
    scripts/build-ios-ffi.sh {{ sdk }} {{ profile }}

# Build the Linux app.
[group('linux')]
linux-build:
    just --justfile clients/qt/justfile build

# Lint and test the Linux app and its release and license tools.
[group('linux')]
linux-check:
    just --justfile clients/qt/justfile check

# Build and test the Windows app on Windows in Git Bash.
[group('windows')]
windows-check:
    just --justfile clients/qt/justfile windows-check

# Lint the runtime for the two Windows targets in Docker on any host.
[group('windows')]
windows-cross-check packages="--workspace":
    docker build --quiet --tag kodosi-windows-cross --file scripts/windows-cross-check.Dockerfile scripts >/dev/null
    for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do \
        docker run --rm --volume "$PWD:/src" --volume kodosi-cargo-registry:/usr/local/cargo/registry --volume kodosi-xwin-cache:/cache/xwin --workdir /src --env CARGO_BUILD_WARNINGS=deny --env CARGO_INCREMENTAL=0 --env KODOSI_GHOSTTY_CHECK_ONLY=1 kodosi-windows-cross bash -euo pipefail -c "\
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target {{ packages }} --lib --bins --examples -- -D warnings && \
            cargo xwin clippy --manifest-path runtime/Cargo.toml --locked --target $target {{ packages }} --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used"; \
    done

# Build the Keycloak theme after the type, lint and format checks.
[group('sign-in')]
sign-in-check:
    cd sign-in && bun install --frozen-lockfile && bun run tsc && bun run lint && bun run format:check && bun run build
