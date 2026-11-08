# mau

A self-hosted personal library and reading tracker. Log a page with one tap,
keep your books in your own order, track the copies you own, and publish the
parts you want to share. Works offline; signs in with a password and, optionally, an authenticator
app.

- What it does and why: [`docs/SPEC.md`](docs/SPEC.md)
- Choices made while building it: [`docs/DECISIONS.md`](docs/DECISIONS.md)
- The interactive design mocks: [`mocks/`](mocks/)

## Building

Requires Rust 1.96+, Node 26+, [just](https://github.com/casey/just), and
`cmake` plus a C compiler (for the TLS library).

    just web-install   # once
    just release       # → target/release/mau, with the web app embedded

The result is one self-contained binary (it needs only the system's C library)
for the build machine's architecture.

## Running in production

mau speaks plain HTTP on localhost. Put a reverse proxy with HTTPS in front of
it; passwords and session cookies must only travel over HTTPS.

1. Create a user and data directory, and copy the config:

       useradd --system --home /var/lib/mau mau
       install -d -o mau -g mau /var/lib/mau
       install -D -m 644 mau.example.toml /etc/mau/mau.toml

   Set `public_url` in `/etc/mau/mau.toml` to the address people will use.
   It appears in setup, invite, and reset links, and names the instance in
   authenticator apps.

2. Start mau (see the service files below) and look at its log. On the first
   start it prints a one-time link:

       Create the admin account: https://books.example.com/setup/…

   Open it to create the admin account. Until an admin exists, a new link is
   printed on every start.

3. In **Settings → Admin → Signup & invites**, choose who can sign up
   (invite-only by default) and create invite links.

### Reverse proxy

Caddy:

    books.example.com {
        reverse_proxy 127.0.0.1:8370
    }

nginx:

    server {
        server_name books.example.com;
        listen 443 ssl;
        # ssl_certificate …; ssl_certificate_key …;
        client_max_body_size 32m;   # imports and cover uploads
        location / {
            proxy_pass http://127.0.0.1:8370;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }

If the proxy isn't on the same machine, add its address to `trusted_proxies`
so rate limits see real client addresses.

### Service files

OpenRC (`/etc/init.d/mau`):

    #!/sbin/openrc-run
    description="mau library and reading tracker"
    command=/usr/local/bin/mau
    command_args="serve"
    command_user="mau:mau"
    command_background=true
    pidfile="/run/mau.pid"
    output_log="/var/log/mau.log"
    error_log="/var/log/mau.log"
    depend() { need net; }

systemd (`/etc/systemd/system/mau.service`):

    [Unit]
    Description=mau library and reading tracker
    After=network.target

    [Service]
    User=mau
    ExecStart=/usr/local/bin/mau serve
    Restart=on-failure

    [Install]
    WantedBy=multi-user.target

### Configuration

`/etc/mau/mau.toml` (or `--config PATH`, or `MAU_CONFIG`). Every key can be
overridden by an environment variable:

| Key | Variable | Default |
|---|---|---|
| `listen` | `MAU_LISTEN` | `127.0.0.1:8370` |
| `public_url` | `MAU_PUBLIC_URL` | `http://localhost:8370` |
| `data_dir` | `MAU_DATA_DIR` | `data` |
| `trusted_proxies` | `MAU_TRUSTED_PROXIES` (comma-separated) | `127.0.0.1, ::1` |
| `backup_keep` | `MAU_BACKUP_KEEP` | `7` |

Logging is set with `MAU_LOG` (for example `MAU_LOG=debug`). Everything else
(signup mode, invites, catalog providers and their API keys) is changed in the
admin pages.

### Forgotten passwords

People who forget their password use one of their recovery codes on the
sign-in page. If they've lost those too, an admin can make a one-time reset
link in **Settings → Admin → Users**. If the admin is the one locked out, run
this on the server (as the `mau` user, with the same config):

    mau reset-password USERNAME

It prints a link that works once, for 24 hours. Using it sets a new password,
signs out the account's other devices, and turns off its two-factor.

### Data and backups

`data_dir` holds:

- `mau.db` — the SQLite database (with `-wal` and `-shm` files while running)
- `covers/` — cover images
- `backups/` — a daily database snapshot, keeping the newest `backup_keep`

For a one-off copy while mau runs: `mau backup /path/to/mau-copy.db`.
Back up `covers/` with your usual tools; the files never change once written.

### Upgrading

Stop mau, replace the binary, start it. Database migrations run on start.

## Development

    just run           # http://localhost:8370, data in ./data
    just dev           # with hot reload on http://localhost:5173
    just seed rri      # add the mocks' sample books to the account "rri"
    just test          # Rust tests, type checks, web unit tests
    just e2e           # browser tests (see below)
    just lint

In development, `public_url` is `http://localhost:8370`, which browsers accept
as a secure context without HTTPS.

The browser tests use Playwright's Chromium.
Install the browser once with `cd web && npx playwright install chromium`.
On Gentoo it also needs `x11-libs/libXcomposite` and `x11-libs/libXdamage`.

API types for the web app are generated from the Rust code by `cargo test`
into `web/src/lib/api/generated/`.

## License

AGPL-3.0-only. See [`LICENSE`](LICENSE).
