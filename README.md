# mau

A self-hosted personal library and reading tracker. See `docs/SPEC.md` for
what it does and `mocks/` for interactive design mocks.

## Building

Requires Rust 1.96+, Node 26+, and [just](https://github.com/casey/just).

    just web-install   # once
    just release       # target/release/mau, with the web app embedded

## Running

    just run           # development: http://localhost:8370, data in ./data
    just dev           # with hot reload on http://localhost:5173

In production, copy `mau.example.toml` to `/etc/mau/mau.toml`, adjust it, run
`mau serve`, and put a reverse proxy with HTTPS in front of it.

## License

AGPL-3.0-only. See `LICENSE`.
