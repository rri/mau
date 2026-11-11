# Deploying maubooks.org

Every push to `main` that passes CI is built as a static x86-64 binary (musl)
and installed on the server. These files are what's on the server; the
workflow is `.github/workflows/ci.yml`.

| File | On the server |
|---|---|
| `mau-deploy` | `/usr/local/sbin/mau-deploy` (root, 755) |
| `sudoers` | `/etc/sudoers.d/mau-deploy` (root, 440) |
| `authorized_keys` | `~deploy/.ssh/authorized_keys` |
| `mau.service` | `/etc/systemd/system/mau.service` |
| `mau.toml` | `/etc/mau/mau.toml` |
| `Caddyfile` | added to `/etc/caddy/Caddyfile` |

## How a deploy works

1. CI runs every test, `cargo audit`, and the musl build.
2. The deploy job checks that its commit is still the newest on `main`, so a
   slow older run never replaces a newer deploy.
3. It joins the tailnet as a short-lived `tag:ci` node, which may reach only
   the server's SSH port, and sends the binary over SSH as the `deploy` user.
4. That key can run only `mau-deploy`, which checks the checksum, stops mau,
   copies the database to `backups/pre-deploy-*.db` (the newest 5 are kept),
   installs the binary (the old one stays as `/usr/local/bin/mau.prev`), and
   starts mau. If `/api/health` doesn't answer within 30 seconds, it puts the
   old binary and the database copy back and starts that.

mau is down for a few seconds during each deploy; the app keeps working
offline and syncs when it's back.

## GitHub settings

The `production` environment (deployments from `main` only) holds:

- `TS_OAUTH_CLIENT_ID`, `TS_OAUTH_SECRET`: a Tailscale OAuth client with the
  Auth Keys (write) scope and tag `tag:ci`
- `DEPLOY_SSH_KEY`: the private half of the key in `authorized_keys`
- `DEPLOY_KNOWN_HOSTS`: the server's SSH host key, as `100.82.93.4 ssh-ed25519 …`

The tailnet policy gives `tag:ci` SSH to the server and nothing else:

```jsonc
"tagOwners": { "tag:ci": ["autogroup:admin"] },
"hosts":     { "cloud-gateway": "100.82.93.4" },
"grants": [{ "src": ["tag:ci"], "dst": ["cloud-gateway"], "ip": ["tcp:22"] }]
```

## Setting up the server (once)

As root, from a checkout of this directory:

```sh
useradd --system --home-dir /mnt/data/mau --no-create-home --shell /usr/sbin/nologin mau
install -d -m 700 -o mau -g mau /mnt/data/mau
install -D -m 644 mau.toml /etc/mau/mau.toml
install -m 644 mau.service /etc/systemd/system/mau.service
systemctl daemon-reload && systemctl enable mau

useradd --system --create-home --home-dir /var/lib/deploy --shell /bin/sh deploy
usermod -p '*' deploy
install -d -m 700 -o deploy -g deploy /var/lib/deploy/.ssh
install -m 600 -o deploy -g deploy authorized_keys /var/lib/deploy/.ssh/authorized_keys
install -m 755 mau-deploy /usr/local/sbin/mau-deploy
install -m 440 sudoers /etc/sudoers.d/mau-deploy && visudo -c

cat Caddyfile >> /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile && systemctl reload caddy
```

The first deploy starts mau; its log has the link for creating the admin:

```sh
journalctl -u mau | grep 'Create the admin account'
```

## By hand

- Roll back to the previous binary: `cp -p /usr/local/bin/mau.prev /usr/local/bin/mau && systemctl restart mau`.
  If the newer version changed the database, stop mau first and copy the
  matching `pre-deploy-*.db` over `/mnt/data/mau/mau.db` (as the `mau` user,
  after deleting `mau.db-wal` and `mau.db-shm`).
- Deploy logs: `journalctl -t mau-deploy`.
- Backups: daily snapshots and pre-deploy copies are in `/mnt/data/mau/backups`,
  on the same volume as the database. Copy them elsewhere for a backup that
  survives losing the volume.
