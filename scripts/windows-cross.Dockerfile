# syntax=docker/dockerfile:1
FROM rust:slim-trixie@sha256:e6cd1d294a300601eaaa4a294b337e5f0826f427fb2d481e9f76e69e7981e244

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates clang cmake lld llvm nasm ninja-build \
    && rm -rf /var/lib/apt/lists/*

ARG RUST_VERSION
RUN rustup toolchain install "$RUST_VERSION" --profile minimal --component clippy,rustfmt,llvm-tools-preview \
        --target x86_64-pc-windows-msvc,aarch64-pc-windows-msvc \
    && rustup default "$RUST_VERSION" \
    && cargo install --locked cargo-xwin --version 0.23.1

ENV CARGO_TARGET_DIR=/src/runtime/target/windows-cross \
    XWIN_CACHE_DIR=/cache/xwin \
    XWIN_ACCEPT_LICENSE=true
