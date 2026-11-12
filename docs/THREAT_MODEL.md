# mau threat model

Version 2, 2026-11-11 (version 1: 2026-11-09). Method: STRIDE per element of
the data-flow diagram, with risks rated by the OWASP Risk Rating Methodology
(likelihood × impact, each Low / Medium / High). Controls are cross-referenced
to OWASP ASVS 4.0 chapters where it helps. Review this document when a trust
boundary, an entry point, or the storage of a secret changes, and at least
once a year.

Version 2 adds the public deployment at https://maubooks.org (GitHub Actions,
Tailscale, the deploy key and script, the shared Hetzner host), and re-rates
every threat against what changed since version 1: the landing page and its
quote endpoint, the signed-in home page, password confirmation for admin
actions, and moving a library between instances (export version 2).

## 1. System overview

mau is a self-hosted personal library and reading tracker: one Rust binary
(Axum, SQLite) serving a SvelteKit single-page app, a JSON API under `/api`,
server-rendered public pages (`/u/{name}`, `/s/{token}`), and cover images
(`/covers/{hash}-{size}.jpg`). It is meant to run behind the operator's own
HTTPS reverse proxy for a small group: an individual, a family, a book club.

**The maubooks.org deployment** (`deploy/README.md`):

- A Hetzner cloud server (Debian 13, x86-64) that also hosts oxo-lang.org,
  a Forgejo instance (git.oxo-lang.org), and a Forgejo Actions runner.
- Caddy terminates TLS (Let's Encrypt; CAA allows Let's Encrypt and ZeroSSL)
  and proxies `maubooks.org` to mau on `127.0.0.1:8370`;
  `www.maubooks.org` redirects.
- mau runs as the `mau` user under a hardened systemd unit
  (`ProtectSystem=strict`, `NoNewPrivileges`, `UMask=0077`) with its data
  in `/mnt/data/mau` (mode 700) on an attached volume. Daily snapshots and
  pre-deploy copies stay on that volume; there is no off-server copy.
- The Hetzner firewall allows only 80 and 443 from the internet. SSH is
  reached over Tailscale.
- The source is public on GitHub. Every push to `main` that passes CI is
  built as a static musl binary and deployed: a short-lived `tag:ci`
  Tailscale node sends it over SSH as the `deploy` user, whose key is
  restricted to running `/usr/local/sbin/mau-deploy` through sudo.

Out of scope: Caddy's and the host OS's own vulnerabilities, Hetzner,
GitHub, and Tailscale as services. Their configuration is in scope where mau
depends on it (§7).

## 2. Assets

| ID | Asset | Why it matters |
|---|---|---|
| A1 | Account credentials: password hashes (Argon2id), recovery-code hashes, TOTP secrets (plain), session-token hashes, reset-link and invite-token hashes | Account takeover |
| A2 | Library data: books, reading history, notes, ratings, tags, copies and their locations | Private by default; notes are never shared |
| A3 | Third-party API keys: the admin's provider keys and users' own Google Books keys (plain) | Quota theft, billing |
| A4 | Admin powers: user management, signup mode, shared catalog, audit log | Instance takeover |
| A5 | The shared catalog (works, editions, covers) | Integrity for everyone on the instance |
| A6 | Availability of the instance, and its standing with Open Library and Google | A small instance can be blocked by providers |
| A7 | Backups in `data_dir/backups`: daily snapshots and pre-deploy copies (copies of A1 to A5) | Same as the live database |
| A8 | Deployment credentials: the GitHub account and `production` environment secrets (Tailscale OAuth client, deploy SSH key, host key), the Tailscale admin account | Whoever holds them can run code on the server |
| A9 | The server itself (root), shared with oxo-lang.org and Forgejo | Everything above, and the other sites |

## 3. Actors

| Actor | Trust | Capabilities |
|---|---|---|
| Anonymous internet user | None | Public pages, the landing page and its quote, sign-in, signup when open or invited, setup and reset links if they hold one |
| Signed-in user | Own data only | Everything under their account; reads the shared catalog; adds works to it via provider results or a mau export's provider links; suggests manual entries |
| Admin | High, within the app | Users, signup, providers, shared catalog, audit log, reset links for others |
| Operator (server root, repo owner) | Full | Config, data directory, `mau reset-password`, `mau backup`, pushes to `main` (which deploy) |
| Catalog providers (Open Library, Google Books, Hardcover, ISBNdb) | Partial: data is used, never executed | Supply titles, people, editions, cover URLs |
| GitHub Actions (runners, third-party actions) | Trusted to build what was pushed | Read the repo; the deploy job holds the `production` secrets |
| Anyone on GitHub | None | Read the source, open pull requests (their workflows run without secrets) |
| Other local accounts and services on the host (Forgejo, its runner's jobs, oxo-lang.org) | None | Whatever file permissions allow |

## 4. Data flow and trust boundaries

```
                    TB1: internet → Caddy (TLS) → 127.0.0.1:8370
 Browser (SPA, IndexedDB, ────────────────┐
 service worker)                          │
   │  JSON /api, cookie mau_session       ▼
   │                        ┌──────────────────────────────┐   TB3: outbound
   ├──── GET /u, /s ───────►│ mau (Axum), user mau         │──────────────────►  Providers
   │                        │  client_ip → rate limits →   │   https only,         (search, editions,
   │                        │  origin check → handlers     │   cover host           covers)
   └──── GET /covers ──────►│  security headers (CSP…)     │   allowlist
                            └──────────────┬───────────────┘
                     TB2: process / disk   │
                            ┌──────────────▼───────────────┐
                            │ /mnt/data/mau (700): mau.db,  │
                            │ covers/, backups/             │
                            └───────────────────────────────┘
 Operator shell ── mau backup / reset-password / config file (TB4: host)

 TB5: CI/CD
 git push ─► GitHub Actions: test, audit, build (musl) ─► deploy job (production
 secrets) ─► Tailscale (tag:ci → server:22 only) ─► sshd: deploy key, forced
 command ─► sudo mau-deploy (root): checksum, stop, copy DB, install, start,
 health check, roll back
```

- **TB1** browser ↔ server: everything a client sends is untrusted. Caddy
  sets `X-Forwarded-For` from the connection (it ignores a client's own
  header unless configured to trust a proxy), and mau trusts it only from
  `127.0.0.1` and `::1`.
- **TB2** server ↔ disk: only the mau account may read the data directory.
- **TB3** server → providers: responses are untrusted input; requests must
  not be steerable to internal hosts.
- **TB4** operator ↔ host: trusted; the setup link and reset links are
  printed only to the server's own log or terminal.
- **TB5** source → server: what lands on `main` runs as `mau` within
  minutes. The deploy path must not give more than that (in particular, not
  root), and must not leak server data back into public CI logs.

## 5. Entry points

| Entry point | Auth | Notes |
|---|---|---|
| `POST /api/auth/*` (login, code, register, recovery, reset, confirm) | None or ticket | Rate-limited per IP and per account |
| `GET /api/quote` | None | A random line from a fixed list compiled into the binary |
| `POST /api/sync` | Session | Up to 500 operations; each is checked for ownership |
| `/api/catalog/*` | Session | Search calls providers (30 a minute per user) |
| `/api/import*` | Session | Bodies up to 30 MB, at most 5,000 rows, one import at a time; mau exports may name provider records |
| `POST /api/covers`, admin cover upload | Session / admin | Bodies up to 10 MB; decoded with caps |
| `/api/admin/*` | Admin | Reset links, deleting users, and granting admin need a recent password confirmation |
| `GET /u/{name}`, `/s/{token}` | None | Server-rendered, escaped by maud |
| `GET /covers/{name}` | None | Names are content hashes; strict pattern |
| Config file and environment | Operator | `public_url` must be https (or localhost) |
| SSH as `deploy` (tailnet only) | Deploy key | `restrict` plus a forced command: only `mau-deploy SHA256 COMMIT` with the binary on stdin |
| Pushes to `main`, `workflow_dispatch` | GitHub write access | Deploy after CI passes |

## 6. Threats and controls (STRIDE)

Risk is the rating *after* the listed controls. Status: ✅ in place,
⚠️ accepted residual risk, 🔭 recommended for later, ❗ open finding (§8).
**Bold** marks what changed in version 2.

### Spoofing

| ID | Threat | Controls | Risk |
|---|---|---|---|
| S1 | Password guessing and credential stuffing | Argon2id (crate defaults: 19 MiB, t=2); minimum 10 characters plus a blocklist of common passwords; 20 attempts a minute per IP and 10 per username; same answer and timing for unknown accounts (dummy hash) ✅ | Low |
| S2 | Guessing authenticator codes after a stolen password | Codes used once (last step stored); 5 tries per sign-in; 10 wrong codes per account within an hour block codes for 15 minutes and leave a warning in the account's recovery activity ✅ | Low |
| S3 | Guessing recovery codes | ~49 bits each, Argon2-hashed, single use; 5 attempts a minute per IP and per username ✅ | Low |
| S4 | Stolen session cookie | 256-bit random token, only its SHA-256 stored; HttpOnly, SameSite=Lax, Secure on https; 30-day sliding expiry; revocable per device; password change and reset end other sessions ✅. No IP or device binding ⚠️ | Low |
| S5 | Setup-link takeover of a fresh instance | Link printed only to the server log; valid until the first admin exists; new link each start ✅. **On maubooks.org the admin was created within minutes of the first deploy, so the link no longer works.** See I9 for a path by which a link could reach a public log | Low |
| S6 | Leaked reset or invite links | Hashed at rest; reset links last 24 hours, single use, only the newest works, and turn two-factor off (shown on the page); invites expire and are single use ✅ | Low |
| S7 | Spoofed client IP to dodge rate limits | `X-Forwarded-For` honored only from `trusted_proxies`, right-most untrusted entry ✅; **Caddy replaces a client's own header: on maubooks.org, 22 failed sign-ins with a different forged `X-Forwarded-For` each were still limited by the real address (2026-11-11)** ✅ | Low |
| **S8** | **Stolen deploy key** | **Usable only from the tailnet (port 22 is closed to the internet); `restrict` (no shell, forwarding, or pty) and a forced command, tested 2026-11-11; the script accepts only a checksum and a commit hash ✅. Whoever has it and tailnet access can still install any binary (see E6)** | Low |
| **S9** | **Stolen Tailscale OAuth client** | **It can only make `tag:ci` nodes, and the tailnet policy lets `tag:ci` reach only the server's SSH port, which still needs the deploy key ✅. Depends on the default allow-all rule having been narrowed (§7) ⚠️** | Low |

### Tampering

| ID | Threat | Controls | Risk |
|---|---|---|---|
| T1 | Cross-site request forgery | SameSite=Lax cookie, plus an exact `Origin` check on every non-GET API request ✅ | Low |
| T2 | Changing another user's records through sync | Every operation checks that the book, reading, copy, or private work belongs to the session's user; each runs in a savepoint ✅ | Low |
| T3 | Malformed sync data (huge IDs, path-like cover names) | IDs of new records ≤ 64 characters `[A-Za-z0-9_-]`; cover overrides must be stored 32-hex hashes; field lengths, dates, pages, ranks, and ratings validated ✅ | Low |
| T4 | Poisoning the shared catalog | Users can add works only from cached provider results; manual entries stay private until an admin promotes them; admin edits are audited ✅. **❗ Since export version 2, importing a mau export adds a book that names a provider record to the shared catalog with the title, people, and editions written in the file, so a crafted file can put made-up works into everyone's catalog (finding V2-1)** | **Medium** |
| T5 | Stored XSS through titles, notes, bios | Svelte and maud escape all text; no `{@html}` or `innerHTML`; strict CSP (scripts from self, plus the shell's inline script by hash) as a second layer ✅ | Low |
| T6 | Clickjacking | `X-Frame-Options: DENY` and `frame-ancestors 'none'` ✅ | Low |
| T7 | Tampering with files on disk by other local users | Data directory forced to mode 700 on every start ✅; **on maubooks.org the other services (Forgejo as `git`, its runner as `runner`, Caddy as `caddy`) are separate users, and the runner's jobs run in containers** ✅ | Low |
| **T8** | **Malicious code reaching `main` (a stolen GitHub session or token, or a mistake)** | **CI must pass first; the `production` secrets reach only jobs on `main`; pull requests from forks run without secrets, and their caches can't be read by `main` ✅. A push runs as `mau` within minutes, with no review step ⚠️ (the chosen trade-off for deploying on every push). GitHub two-factor and branch protection against force-pushes are the operator's 🔭** | Medium |
| **T9** | **Compromised build dependency or GitHub action** | **Lock files committed and used (`npm ci`, `--locked`); third-party actions pinned to commits; Rust pinned to 1.96.0; `cargo audit` on every run ✅. `npm audit` isn't automated, and advisories published between pushes go unnoticed until the next push 🔭** | Medium |

### Repudiation

| ID | Threat | Controls | Risk |
|---|---|---|---|
| R1 | An admin denies an action | Audit log of admin and account-level actions, kept forever ✅. The log lives in the same database an admin can't edit through the app, but the operator can ⚠️ | Low |
| R2 | A user doesn't notice account misuse | Recovery activity lists recovery-code use, reset links, and code lockouts; Sessions lists devices and last use ✅. No email or push notifications ⚠️ | Medium |
| **R3** | **Not knowing what was deployed when** | **Each deploy is logged with its commit (`journalctl -t mau-deploy`), and GitHub keeps the run, its tests, and its artifact for 30 days ✅** | Low |

### Information disclosure

| ID | Threat | Controls | Risk |
|---|---|---|---|
| I1 | Reading another user's library | All reads scoped to the session's user; private works visible only to their owner ✅ | Low |
| I2 | Account enumeration | Sign-in gives one answer for unknown or wrong; profiles with nothing published look like unknown names ✅. Username availability check and signup reveal taken names (needed for signup; 60 a minute per IP) ⚠️ | Low |
| I3 | Secrets readable on disk | Directory mode 700 ✅. TOTP secrets and API keys stored in plain text (needed to use them) ⚠️; full-disk or volume encryption is the operator's 🔭. **The Hetzner volume isn't encrypted by mau or the operator** ⚠️ | Medium |
| I4 | Backups leaking | Kept inside the private data directory ✅; **on maubooks.org there are no copies elsewhere** (so nothing else to leak; see D6) | Low |
| I5 | Data left on a shared browser | Sign-out deletes the device's IndexedDB copy ✅. Signing out while offline leaves the server session alive until it expires ⚠️ | Low |
| I6 | Tracking through third-party requests | Covers are downloaded by the server and served from mau; the browser never contacts providers; **the landing page's fonts are bundled**; `Referrer-Policy: same-origin` ✅ | Low |
| I7 | Public pages exposing too much | Only published items; notes never; ratings only if chosen ✅ | Low |
| I8 | SQLite stale-memory bug (RUSTSEC-2026-0459, CVE-2026-119375) in the bundled SQLite | Needs a crafted `.db` + `-wal` pair placed where mau opens it; blocked by the private data directory ⚠️. Upgrade when sqlx allows `libsqlite3-sys` ≥ 0.39 🔭. **Ignored by name in CI's `cargo audit`, so any other advisory still fails the build** | Low |
| **I9** | **Server log lines in public CI logs** | **❗ When a deploy fails its health check, `mau-deploy` prints the last 30 lines of mau's log over SSH, and the public Actions log keeps them. On an instance without an admin those lines include the setup link; otherwise they can include errors and usernames (finding V2-3)** | **Medium** |
| **I10** | **Deployment details in the public repository** | **`deploy/` and the workflow name the tailnet address, host key, paths, and users. Reaching any of them needs the tailnet and the deploy key ⚠️** | Low |

### Denial of service

| ID | Threat | Controls | Risk |
|---|---|---|---|
| D1 | Decompression-bomb images | Uploads ≤ 10 MB; decoding capped at 10,000 px a side and 256 MB, two at a time ✅ | Low |
| D2 | Runaway imports exhausting memory or provider goodwill | 30 MB bodies, 5,000 rows, 5 starts a minute, one import in progress per user, 3 provider requests at a time, 30-day provider cache ✅. **A mau export adds one Open Library request per linked book while matching (a 1,028-book library: 5 minutes to match, 9 to save)** | Low |
| D3 | Argon2 CPU exhaustion via sign-in endpoints | Per-IP limits ✅; a distributed attack could still load the CPU ⚠️ (proxy-level limits 🔭). **Now public, on a 2-core server shared with other sites** | Medium |
| D4 | Locking someone out by failing their sign-ins | Per-username limit blocks password attempts for a minute; code lockout lasts 15 minutes and needs the right password first ✅ | Low |
| D5 | Large sync batches | 500 operations per request, default 2 MB request bodies elsewhere ✅ | Low |
| **D6** | **Losing the data volume (failure, deletion, or a mistake)** | **Daily snapshots (7) and pre-deploy copies (5) protect against bad deploys and bad edits ✅, but they sit on the same volume as the database, with no off-server copy and no Hetzner backups ⚠️ (operator's decision, 2026-11-11)** | **Medium** |
| **D7** | **A bad deploy taking the site down** | **CI runs every test layer first; `mau-deploy` copies the database, checks `/api/health` for 30 seconds, and otherwise restores the previous binary and the database copy (tested); a run whose commit is no longer the newest on `main` doesn't deploy ✅. A few seconds of downtime per deploy ⚠️** | Low |

### Elevation of privilege

| ID | Threat | Controls | Risk |
|---|---|---|---|
| E1 | Non-admin reaching admin endpoints | `Admin` extractor on every admin handler; invites limited to the creator's own unless admin ✅ | Low |
| E2 | Removing the last admin | Refused for demote, suspend, and delete ✅ | Low |
| E3 | Sensitive changes from a stolen session | Password, username, two-factor, recovery codes, and account deletion need a password (and code) confirmation within 10 minutes ✅; **so do an admin's reset links for others, deleting users, and granting admin** ✅. The Google Books key does not (decision 2026-11-08) ⚠️ | Low |
| E4 | Server-side request forgery through cover URLs | https only; host allowlist of provider image hosts; redirects only to https domain names (no IP literals or localhost), at most five; responses read with a 10 MB cap ✅. **A mau export's provider IDs are put into Open Library and Google URLs unchecked; the hosts are fixed and the allowlist still applies, so at most a query on those hosts can be altered (finding V2-4)** | Low |
| E5 | Supply-chain compromise of dependencies | See T9: **`cargo audit` now runs in CI** ✅; npm has 4 low dev-only advisories (deepmerge, through `@sveltejs/kit`'s build tooling; none at runtime) ⚠️ | Medium |
| **E6** | **The deploy path giving more than `mau`'s rights** | **The binary runs as `mau` under systemd hardening, and the database copy runs the old binary as `mau` ✅. ❗ But `mau-deploy` runs the uploaded binary's `--version` check as root, so the deploy key or a malicious push gets root on the server, and with it oxo-lang.org and Forgejo (finding V2-2)** | **Medium** |
| **E7** | **Another service on the shared host reaching mau** | **Separate users, mau's data directory 700, systemd's `ProtectSystem=strict`/`ProtectHome`, Forgejo registration closed, runner jobs in containers ✅. A local privilege escalation in any of them reaches everything ⚠️. Forgejo listens on all addresses (shielded only by the Hetzner firewall) and sshd allows passwords for non-root users 🔭** | Low |

## 7. Assumptions and operator responsibilities

- HTTPS terminates at a proxy the operator controls; `public_url` is https.
  mau sends HSTS (one year, no `includeSubDomains`, no preload).
- The proxy passes the real client address in `X-Forwarded-For`, and its
  address is in `trusted_proxies`.
- The service runs as its own `mau` user with the hardening in README. **The
  maubooks.org host is not single-purpose (§1); the other services must stay
  under their own users.**
- Backups copied off the host are encrypted by the operator. **(maubooks.org
  has none; see D6.)**
- Admins are trusted with everyone's account recovery (reset links) but not
  with reading private libraries through the app; the operator can read all.
- **The GitHub account that owns the repository uses two-factor
  authentication, and the `production` environment stays limited to `main`.**
- **The tailnet policy's default allow-all rule has been narrowed (for
  example to `autogroup:member`), so `tag:ci` nodes reach only the server's
  SSH port. This was asked of the operator on 2026-11-11 but hasn't been
  verified.**
- **The Hetzner firewall keeps SSH closed to the internet.**

## 8. Recommendations not yet done

Findings from the 2026-11-11 review, most important first:

1. **V2-1 (T4):** build shared works from the provider's own answer, not the
   export: look the work up by its ID (Open Library: the search by key that
   import already makes for the cover; Google: the volume, with the user's
   key) and fall back to normal matching when that fails. Accept only
   well-formed provider names and IDs.
2. **V2-2 (E6):** run the `--version` check as `mau` (`runuser -u mau`), so
   nothing uploaded ever runs as root.
3. **V2-3 (I9):** don't print mau's log over SSH on a failed deploy; say
   where to look (`journalctl -u mau`) instead.
4. **V2-4 (E4):** validate provider IDs (`OL…W`, Google's
   `[A-Za-z0-9_-]{12}`) before using them in URLs; covered by item 1.
5. Run CI weekly on a schedule as well, so `cargo audit` catches advisories
   between pushes; add `npm audit --omit=dev` (T9, E5).
6. Upgrade SQLite once sqlx accepts `libsqlite3-sys` ≥ 0.39 (I8).
7. For the operator: an off-server backup copy (D6); GitHub two-factor and
   branch protection against force-pushes and deletion (T8); confirm the
   tailnet policy (S9); bind Forgejo to localhost and turn off SSH password
   logins (E7).
8. Optional proxy-level rate limiting in Caddy (D3).
9. Consider encrypting TOTP secrets and API keys with a key kept outside the
   database, if backups are stored where others can read them (I3).
10. Queue a server sign-out when signing out offline (I5).

## Appendix A: review of 2026-11-11

Reviewed: everything that changed since version 1 (26 commits): the landing
page and `GET /api/quote`, the signed-in home page, admin password
confirmation, the import and export changes for moving between instances, the
CI workflow, the deploy script, sudoers and SSH key restrictions, the systemd
unit, the Caddy configuration, and the shared host. Checks: 175 server tests,
207 web tests, 5 Playwright tests, the Firefox suite; on the server, the deploy
key tried with other commands, a shell, command injection, and port
forwarding (all refused), a wrong checksum (refused), and the deploy script's
rollback in a sandbox; `cargo audit` (clean apart from I8) and `npm audit` (4
low, dev-only).

| # | Finding | Severity | Outcome |
|---|---|---|---|
| V2-1 | A crafted mau export can add made-up works to the shared catalog (T4) | Medium | Open: fix proposed (§8.1) |
| V2-2 | The deploy script runs the uploaded binary as root to check it (E6) | Medium | Open: fix proposed (§8.2) |
| V2-3 | A failed deploy copies server log lines, possibly a setup link, into public CI logs (I9) | Medium | Open: fix proposed (§8.3) |
| V2-4 | Provider IDs from an export used in URLs unchecked (E4) | Low | Open: covered by V2-1's fix |
| V2-5 | Advisories between pushes go unnoticed; `npm audit` not automated (T9) | Low | Open (§8.5) |
| V2-6 | Backups only on the same volume as the database (D6) | Medium | Accepted by the operator |
| V2-7 | Forgejo on all addresses; sshd password logins on (E7) | Low | Operator (§8.7) |
| V2-8 | The AAAA records pointed at the subnet's router address, which answered only because IPv6 forwarding was on | Low (availability) | Fixed by the operator before launch |

## Appendix B: audit of 2026-11-09

Reviewed: authentication and sessions, sync authorization, catalog and
provider calls, covers, import, export, sharing and public pages, admin
endpoints, security headers, file permissions, the web client's HTML sinks
and offline storage, and dependency advisories. Functional checks: 157 server
tests, the web unit tests, and Firefox runs of the reading, catalog, add,
detail, sharing, import, settings, admin, and sign-in / two-factor / recovery
/ reset flows.

| # | Finding | Severity | Outcome |
|---|---|---|---|
| 1 | Data directory and database created world-readable (755 / 644) | Medium | Fixed: made 700 on start; README uses `install -m 700`, `UMask=0077` |
| 2 | Image decoding had no dimension or memory cap | Medium | Fixed (D1) |
| 3 | No Content-Security-Policy | Medium | Fixed (T5) |
| 4 | Authenticator codes guessable across many sign-ins (~50 a minute per account) | Medium | Fixed (S2), with a warning to the account owner |
| 5 | Password change left other sessions signed in | Low | Fixed (S4) |
| 6 | Unlimited parallel imports per user | Low | Fixed (D2) |
| 7 | Cover downloads followed any redirect and read bodies without a cap when no length was sent | Low | Fixed (E4) |
| 8 | Sync accepted any string as a cover override and unbounded IDs | Low | Fixed (T3) |
| 9 | Empty profiles revealed names and bios | Low | Fixed (I2) |
| 10 | No HSTS header | Low | Fixed when `public_url` is https |
| 11 | Bundled SQLite advisory RUSTSEC-2026-0459 | Low (local) | Mitigated by finding 1; upgrade pending (I8) |
| 12 | Dev-only npm advisory (deepmerge prototype poisoning) | Info | Build-time only; no action |
| 13 | Book page "‹ Catalog" link styled as italic grey text (CSS class clash) | Bug | Fixed |
| 14 | On phones, starting a book or editing pages near the list's end opened the form behind the tab bar | Bug | Fixed |
