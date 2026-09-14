set shell := ["bash", "-euo", "pipefail", "-c"]

cargo_tools_root := justfile_directory() + "/.tools/rust"
cargo_tools_bin := cargo_tools_root + "/bin"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/../kodosi-ghostty")

export PATH := justfile_directory() + "/dotnetBackend/.tools/dotnet-10.0.401:" + cargo_tools_bin + ":" + env('PATH')
export KODOSI_GHOSTTY_DIR := ghostty_dir

default:
    @just --list

rust-tools-install:
    cargo install --locked --root {{ cargo_tools_root }} cargo-deny --version 0.19.0
    cargo install --locked --root {{ cargo_tools_root }} cargo-machete --version 0.9.1
    cargo install --locked --root {{ cargo_tools_root }} cargo-llvm-cov --version 0.8.5

dev-tools-install: rust-tools-install
    cargo install --locked --root {{ cargo_tools_root }} bacon --version 3.22.0

_dependency-tools:
    @test -x {{ cargo_tools_bin }}/cargo-deny -a -x {{ cargo_tools_bin }}/cargo-machete || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

_coverage-tool:
    @test -x {{ cargo_tools_bin }}/cargo-llvm-cov || { printf '%s\n' 'Run just rust-tools-install first.' >&2; exit 1; }

rust-pin-parity:
    #!/usr/bin/env bash
    set -euo pipefail
    package_commit=""
    macos_upstream_commit=""
    linux_upstream_commit=""
    while IFS='=' read -r key value; do
        case "$key" in
            package_commit) package_commit="$value" ;;
            upstream_commit) macos_upstream_commit="$value" ;;
            linux_upstream_commit) linux_upstream_commit="$value" ;;
        esac
    done < Ghostty.lock
    test -n "$package_commit"
    test -n "$macos_upstream_commit"
    test -n "$linux_upstream_commit"
    test "$package_commit" = "$(git -C "$KODOSI_GHOSTTY_DIR" rev-parse HEAD)"
    test "$macos_upstream_commit" = "$(tr -d '[:space:]' < "$KODOSI_GHOSTTY_DIR/MacOSGhostty.ref")"
    test "$linux_upstream_commit" = "$(tr -d '[:space:]' < "$KODOSI_GHOSTTY_DIR/LinuxGhostty.ref")"
    bash scripts/verify-ghostty-lock-parity.sh
    bash scripts/test-ghostty-lock-parity.sh

rust-build:
    cargo build --manifest-path rustProcess/Cargo.toml --locked --workspace

rust-cli-build:
    cargo build --manifest-path rustProcess/Cargo.toml --locked --bin kodosi

rust-ffi-build:
    if [[ "$(uname -s)" == Darwin ]]; then export MACOSX_DEPLOYMENT_TARGET=26.4; fi; python3 scripts/build-ffi-artifact.py --manifest rustProcess/Cargo.toml --output-dir rustProcess/target/ffi-artifacts --profile release --target "$(rustc -vV | grep '^host:' | cut -d' ' -f2)"

rust-ffi-debug-build:
    if [[ "$(uname -s)" == Darwin ]]; then export MACOSX_DEPLOYMENT_TARGET=26.4; fi; python3 scripts/build-ffi-artifact.py --manifest rustProcess/Cargo.toml --output-dir rustProcess/target/ffi-artifacts --profile ffi-debug --target "$(rustc -vV | grep '^host:' | cut -d' ' -f2)"

rust-check: rust-feature-gates
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path rustProcess/Cargo.toml --locked --workspace

rust-feature-gates:
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path rustProcess/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib
    CARGO_BUILD_WARNINGS=deny cargo test --manifest-path rustProcess/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib --no-run
    CARGO_BUILD_WARNINGS=deny cargo check --manifest-path rustProcess/Cargo.toml --locked -p kodosi-ffi-c

rust-fmt:
    cargo fmt --manifest-path rustProcess/Cargo.toml --all -- --check

rust-doc:
    CARGO_BUILD_WARNINGS=deny RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path rustProcess/Cargo.toml --locked --workspace --no-deps

rust-clippy:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path rustProcess/Cargo.toml --locked --workspace --lib --bins --examples -- -D warnings
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path rustProcess/Cargo.toml --locked --workspace --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used

rust-deny: _dependency-tools
    cargo deny --manifest-path rustProcess/Cargo.toml --workspace --locked check --config deny.toml --hide-inclusion-graph advisories bans licenses sources

rust-machete: _dependency-tools
    cargo machete --skip-target-dir rustProcess

rust-dependency-hygiene: rust-deny rust-machete

ffi-publication-test:
    PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-ffi-artifact.py

rust-test: ffi-publication-test
    CARGO_INCREMENTAL=0 cargo test --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets
    CARGO_INCREMENTAL=0 cargo test --manifest-path rustProcess/Cargo.toml --locked --workspace --doc

rust-coverage: _coverage-tool
    cargo llvm-cov --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --html --output-dir rustProcess/target/llvm-cov

rust-coverage-lcov: _coverage-tool
    mkdir -p rustProcess/target/llvm-cov
    cargo llvm-cov --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --lcov --output-path rustProcess/target/llvm-cov/lcov.info

rust-coverage-check: _coverage-tool
    mkdir -p rustProcess/target/llvm-cov
    cargo llvm-cov --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' --fail-under-lines 55 --json --summary-only --output-path rustProcess/target/llvm-cov/summary.json

protocol-gen:
    cargo run --manifest-path rustProcess/Cargo.toml --locked --bin protocol-gen -- write

protocol-check:
    cargo run --manifest-path rustProcess/Cargo.toml --locked --bin protocol-gen -- check

rust-all: rust-pin-parity rust-fmt rust-check rust-doc rust-clippy rust-dependency-hygiene protocol-check rust-test rust-coverage-check

rust-clean:
    cargo clean --manifest-path rustProcess/Cargo.toml

backend-restore:
    cd dotnetBackend && dotnet restore Kodosi.slnx --locked-mode

backend-build: backend-restore
    cd dotnetBackend && dotnet build Kodosi.slnx --no-restore

backend-test: backend-build
    cd dotnetBackend && dotnet test --solution Kodosi.slnx --no-restore --no-build

backend-run: backend-restore
    cd dotnetBackend && dotnet run --no-restore --project src/Kodosi.Host

backend-docker-up:
    docker compose up --build --detach --wait

backend-docker-down:
    docker compose down

backend-docker-logs:
    docker compose logs --follow backend

backend-all: backend-test

check-all: rust-all backend-all
