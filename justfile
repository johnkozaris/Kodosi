cargo_tools_root := "rustProcess/target/rust-tools"
cargo_tools_bin := "rustProcess/target/rust-tools/bin"
ghostty_dir := env('KODOSI_GHOSTTY_DIR', justfile_directory() + "/../kodosi-ghostty")

export PATH := cargo_tools_bin + ":" + env('PATH')
export KODOSI_GHOSTTY_DIR := ghostty_dir

rust-pin-parity:
    #!/usr/bin/env bash
    set -euo pipefail
    package_commit="$(awk -F= '$1 == "package_commit" { print $2 }' Ghostty.lock)"
    test -n "$package_commit"
    test "$package_commit" = "$(git -C "$KODOSI_GHOSTTY_DIR" rev-parse HEAD)"
    swift_dir="${KODOSI_SWIFT_DIR:-$(pwd)/../kodosiSwift}"
    if [[ ! -d "$swift_dir" ]]; then
        echo "Required sibling Swift checkout is missing: $swift_dir" >&2
        exit 1
    fi
    bash scripts/verify-ghostty-lock-parity.sh
    bash scripts/test-ghostty-lock-parity.sh

rust-tools-install:
    cargo install --locked --root {{ cargo_tools_root }} cargo-nextest --version 0.9.133
    cargo install --locked --root {{ cargo_tools_root }} cargo-deny --version 0.19.0
    cargo install --locked --root {{ cargo_tools_root }} cargo-machete --version 0.9.1
    cargo install --locked --root {{ cargo_tools_root }} cargo-llvm-cov --version 0.8.5

dev-tools-install:
    just rust-tools-install
    cargo install --locked --root {{ cargo_tools_root }} bacon --version 3.22.0

_rust-tools-guard:
    @test -x {{ cargo_tools_bin }}/cargo-nextest || just rust-tools-install

_clippy manifest:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path {{ manifest }} --locked --workspace --lib --bins --examples
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo clippy --manifest-path {{ manifest }} --locked --workspace --tests -- \
        -A clippy::unwrap_used \
        -A clippy::expect_used

_coverage out_args:
    cargo llvm-cov nextest --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets --ignore-filename-regex '(^|/)(tests|benches|examples)/' {{ out_args }}

rust-build:
    cargo build --manifest-path rustProcess/Cargo.toml --locked --workspace

rust-cli-build:
    cargo build --manifest-path rustProcess/Cargo.toml --locked --bin kodosi

rust-ffi-build:
    if [[ "$(uname -s)" == "Darwin" ]]; then MACOSX_DEPLOYMENT_TARGET=26.4 cargo build --manifest-path rustProcess/Cargo.toml --locked --release -p kodosi-ffi-c; else cargo build --manifest-path rustProcess/Cargo.toml --locked --release -p kodosi-ffi-c; fi
    @echo
    @echo "  staticlib: rustProcess/target/release/libkodosi_ffi_c.a"
    @echo "  header:    rustProcess/target/include/kodosi_runtime.h"

rust-ffi-debug-build:
    if [[ "$(uname -s)" == "Darwin" ]]; then MACOSX_DEPLOYMENT_TARGET=26.4 cargo build --manifest-path rustProcess/Cargo.toml --locked --profile ffi-debug -p kodosi-ffi-c; else cargo build --manifest-path rustProcess/Cargo.toml --locked --profile ffi-debug -p kodosi-ffi-c; fi
    @echo
    @echo "  debug artifacts: rustProcess/target/ffi-debug/"

rust-check: rust-feature-gates
    cargo check --manifest-path rustProcess/Cargo.toml --locked --workspace

rust-feature-gates:
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo check --manifest-path rustProcess/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo test --manifest-path rustProcess/Cargo.toml --locked -p kodosi-runtime --no-default-features --lib --no-run
    CARGO_BUILD_WARNINGS=deny CARGO_INCREMENTAL=0 cargo check --manifest-path rustProcess/Cargo.toml --locked -p kodosi-ffi-c

rust-fmt:
    cargo fmt --manifest-path rustProcess/Cargo.toml --all -- --check

rust-doc:
    CARGO_BUILD_WARNINGS=deny RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path rustProcess/Cargo.toml --locked --workspace --no-deps

rust-clippy: (_clippy "rustProcess/Cargo.toml")

rust-deny:
    cargo deny --manifest-path rustProcess/Cargo.toml --workspace --locked check --config deny.toml --hide-inclusion-graph advisories bans licenses sources

rust-machete:
    cargo machete --skip-target-dir rustProcess

rust-dependency-hygiene: _rust-tools-guard rust-deny rust-machete

rust-test: _rust-tools-guard
    cargo build --manifest-path rustProcess/Cargo.toml --locked -p kodosi-ffi-c
    CARGO_INCREMENTAL=0 cargo nextest run --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets

    CARGO_INCREMENTAL=0 cargo test --manifest-path rustProcess/Cargo.toml --locked --workspace --doc

rust-terminal-bench-check:
    cargo clippy --manifest-path rustProcess/Cargo.toml --locked -p ghostty-vt --lib --tests --benches -- \
        -A clippy::unwrap_used \
        -A clippy::expect_used

rust-pq-vectors:
    KODOSI_EMIT_PQ_VECTORS=1 cargo nextest run --manifest-path rustProcess/Cargo.toml --locked -p kodosi-runtime --nocapture emit_wire_format_parity_vectors

rust-coverage: (_coverage "--html --output-dir rustProcess/target/llvm-cov")
    cargo llvm-cov clean --manifest-path rustProcess/Cargo.toml --workspace

rust-coverage-lcov: (_coverage "--lcov --output-path rustProcess/target/llvm-cov/lcov.info")
    cargo llvm-cov clean --manifest-path rustProcess/Cargo.toml --workspace

rust-console-check:
    cd rustProcess && RUSTFLAGS="--cfg tokio_unstable" cargo check --locked -p kodosi-runtime --features tokio-console

rust-console-run *args:
    cd rustProcess && RUSTFLAGS="--cfg tokio_unstable" cargo run --locked -p kodosi-runtime --features tokio-console --bin kodosi -- {{ args }}

rust-arch-gates:
    ./scripts/arch-gates.sh

rust-coverage-check: _rust-tools-guard
    cargo build --manifest-path rustProcess/Cargo.toml --locked -p kodosi-ffi-c
    CARGO_INCREMENTAL=0 cargo llvm-cov nextest --manifest-path rustProcess/Cargo.toml --locked --workspace --all-targets \
        --ignore-filename-regex '(^|/)(tests|benches|examples)/' \
        --fail-under-lines 55 \
        --summary-only
    cargo llvm-cov clean --manifest-path rustProcess/Cargo.toml --workspace



rust-all: rust-pin-parity rust-feature-gates rust-fmt rust-doc rust-clippy rust-terminal-bench-check rust-dependency-hygiene rust-arch-gates protocol-check protocol-compat rust-test rust-coverage-check
    just rust-target-limit

rust-target-size:
    #!/usr/bin/env bash
    set -euo pipefail
    target="rustProcess/target"
    if [[ ! -d "$target" ]]; then
        echo "Rust target: 0 GiB (not created)"
        exit 0
    fi
    if ! size_output="$(du -sk "$target" 2>/dev/null)"; then
        echo "warning: Rust target size changed during measurement; retry 'just rust-target-size'" >&2
        exit 0
    fi
    kib="$(awk 'END { print $1 }' <<<"$size_output")"
    if [[ -z "$kib" ]]; then
        echo "warning: Rust target size changed during measurement; retry 'just rust-target-size'" >&2
        exit 0
    fi
    gib="$(awk -v kib="$kib" 'BEGIN { printf "%.1f", kib / 1048576 }')"
    echo "Rust target: ${gib} GiB"
    for path in "$target/debug/deps" "$target/debug/incremental" "$target/llvm-cov-target" "$target/release"; do
        [[ -e "$path" ]] && { du -sh "$path" 2>/dev/null || true; }
    done
    if (( kib > 20 * 1024 * 1024 )); then
        echo "warning: Rust target exceeds 20 GiB; run 'just rust-clean' after a toolchain, profile, or dependency migration" >&2
    fi

rust-target-limit:
    #!/usr/bin/env bash
    set -euo pipefail
    target="rustProcess/target"
    limit_kib=$((20 * 1024 * 1024))
    [[ -d "$target" ]] || exit 0
    kib="$(du -sk "$target" | awk '{print $1}')"
    if (( kib > limit_kib )); then
        echo "Rust target is $((kib / 1024 / 1024)) GiB; cleaning the 20 GiB cache limit"
        cargo clean --manifest-path rustProcess/Cargo.toml
    fi

rust-clean:
    cargo clean --manifest-path rustProcess/Cargo.toml

rust-clean-debug:
    cargo clean --manifest-path rustProcess/Cargo.toml --profile dev

rust-clean-release:
    cargo clean --manifest-path rustProcess/Cargo.toml --release

protocol-gen:
    cargo run --manifest-path rustProcess/Cargo.toml --locked --bin protocol-gen -- write

protocol-check:
    cargo run --manifest-path rustProcess/Cargo.toml --locked --bin protocol-gen -- check

protocol-compat:
    cargo run --manifest-path rustProcess/Cargo.toml --locked --bin protocol-gen -- check-compat

backend-restore:
    cd dotnetBackend && dotnet tool restore
    cd dotnetBackend && dotnet restore --locked-mode

backend-build: backend-restore
    cd dotnetBackend && dotnet build --no-restore

backend-test: backend-build
    cd dotnetBackend && dotnet test --no-restore --no-build

backend-run: backend-restore
    cd dotnetBackend && dotnet run --no-restore --project src/Kodosi.Host

backend-docker-up:
    docker compose up --build --detach --wait

backend-docker-down:
    docker compose down

backend-docker-logs:
    docker compose logs --follow backend

backend-docker-reset:
    docker compose down --volumes

backend-all: backend-test

check-all: rust-all backend-all

install: dev-tools-install
