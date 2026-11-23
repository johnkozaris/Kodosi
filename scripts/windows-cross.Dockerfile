# syntax=docker/dockerfile:1.29.0@sha256:572cebbc2b15e6f84c077742952674889cb63cd5e9b80abdffcbe587b6f19183
FROM rust:slim-trixie@sha256:6f14b2b4b278ec0d50aa1d69541c304ce0f95e0f6268487aa9924af1c2e9aebe

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates clang cmake lld llvm nasm ninja-build \
    && rm -rf /var/lib/apt/lists/*

ARG RUST_VERSION
RUN rustup toolchain install "$RUST_VERSION" --profile minimal --component clippy,rustfmt,llvm-tools-preview \
        --target x86_64-pc-windows-msvc,aarch64-pc-windows-msvc \
    && rustup default "$RUST_VERSION" \
    && cargo install cargo-xwin --version 0.23.1

ENV CARGO_TARGET_DIR=/src/runtime/target/windows-cross \
    XWIN_CACHE_DIR=/cache/xwin \
    XWIN_ACCEPT_LICENSE=true
# cargo-xwin keeps the CRT and SDK of the cache that it filled, so other versions take effect
# after docker volume rm kodosi-xwin-cache.
ENV XWIN_VERSION=17 \
    XWIN_SDK_VERSION=10.0.26100 \
    XWIN_CRT_VERSION=14.44.17.14
