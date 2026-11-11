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

# Run all tests (except end-to-end; see `just e2e`)
test:
    cargo test --workspace
    cd web && npm run check && npx vitest --run

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

# End-to-end tests (needs `npx playwright install chromium` once)
e2e: web-build
    cargo build -p mau-server
    cd web && npx playwright test

# Browser tests in headless Firefox against throwaway servers (`just browser-test app` for one suite)
browser-test suite="": web-build
    cargo build -p mau-server
    cd web && node browser-tests/run.mjs {{suite}}

# Development: add the mock sample books to an account (e.g. `just seed rri`)
seed user:
    MAU_DATA_DIR=data cargo run -p mau-server -- seed --user {{user}}
