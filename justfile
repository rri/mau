# mau development tasks. Run `just` to list them.

default:
    @just --list

# Install web dependencies
web-install:
    cd web && npm ci

# Build the web app into web/build (embedded into the binary)
web-build:
    cd web && npm run build

# Run the server in development (serves the last web build)
run: web-build
    MAU_DATA_DIR=data cargo run -p mau-server -- serve

# Live development: Rust server on :8370, Vite with hot reload on :5173
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    MAU_DATA_DIR=data cargo run -p mau-server -- serve &
    (cd web && npm run dev) &
    wait

# Run all tests
test:
    cargo test --workspace
    cd web && npm run check

# Check formatting and lints
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cd web && npm run lint

# Format everything
fmt:
    cargo fmt --all
    cd web && npm run format

# Release binary at target/release/mau, with the web app embedded
release: web-build
    cargo build --release -p mau-server
    @ls -lh target/release/mau
