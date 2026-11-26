# mau threat model

Version 3, 2026-11-25 (version 2: 2026-11-11; version 1: 2026-11-09). Method: STRIDE per element of
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

Version 3 adds access for other programs: personal API tokens, "Sign in with
mau" (OAuth 2.1 with PKCE, apps known by their address, no registration), the
public API at `/api/v1`, CORS for it, the server fetching apps' metadata
documents, and Admin › Apps. New rows are S10–S13, T10–T13, R4, I11–I14,
D8–D10 and E8–E11; the review behind them is Appendix C.

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
| A10 | API access: personal tokens, apps' access and refresh tokens (hashes), app connections (grants), OAuth requests and codes (server memory) | Whoever holds a token acts as the reader within its scopes, without a password or two-factor |

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
| Third-party app (any website or program) | None until a reader allows it, then that reader's scopes | Send readers to `/oauth/authorize` with any address as `client_id`; exchange codes; call `/api/v1` from any origin; serve a metadata document mau fetches |
| Holder of a token (a script, an app, or whoever copied it) | The token's scopes, for its owner | `/api/v1` only; never the account, tokens, imports, invites or admin |
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
- **TB6** other programs ↔ server: apps and scripts on any origin, holding
  bearer tokens. They reach only `/api/v1` and the OAuth token and revoke
  endpoints, which never read cookies; the cookie API sends no CORS headers.
  What an app says about itself (its name, extra redirect addresses) is
  untrusted; only its address (origin) identifies it.
- **TB7** server → apps' addresses: fetching a metadata document is an
  outbound request to an address anyone can choose, so it must not reach the
  server's own network.

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
| `GET /oauth/authorize` | None | Validates the app's address and return address before anything else (bad ones: an error page, never a redirect); may fetch the app's metadata (TB7); 60 a minute per address |
| `/api/oauth/requests/{id}` (view, approve, deny) | Session | Same-origin cookie API with the `Origin` check; approving needs a recent password confirmation |
| `POST /oauth/token`, `POST /oauth/revoke` | PKCE verifier, refresh token | Any origin (CORS `*`, no credentials); form bodies; 60 a minute per address |
| `GET /.well-known/oauth-authorization-server` | None | Static discovery document |
| `/api/v1/*` | Bearer token | Bearer only (cookies ignored); scopes per endpoint and field; 120 a minute per token; writes go through `library::apply` |
| `/api/api-access*`, `/api/admin/apps*` | Session / admin | Making tokens needs a recent confirmation; no token reaches these |
| Config file and environment | Operator | `public_url` must be https (or localhost) |
| SSH as `deploy` (tailnet only) | Deploy key | `restrict` plus a forced command: only `mau-deploy SHA256 COMMIT` with the binary on stdin |
| Pushes to `main`, `workflow_dispatch` | GitHub write access | Deploy after CI passes |

## 6. Threats and controls (STRIDE)

Risk is the rating *after* the listed controls. Status: ✅ in place,
⚠️ accepted residual risk, 🔭 recommended for later, ❗ open finding (§8).
**Bold** marks what changed in version 2; rows new in version 3 are listed in the header.

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
| S10 | A phishing app: a link to mau's real consent screen for an app that isn't what it claims | The screen names the app by its host, which is where the code goes (redirects only to the app's own origin, or loopback and app schemes its own address lists); the app's self-chosen name only appears beside it as "calls itself"; hosts are shown in punycode (url crate), so look-alike Unicode domains look different; an app on the reader's computer is called out; admins can block an address ✅. A reader who allows a lookalike domain gives it access, as with any "Sign in with…" screen ⚠️ | Medium |
| S11 | Apps sharing one origin (e.g. `https://host.example/~alice/` and `/~bob/`) impersonating each other | Identity is the origin by design: two apps on one origin can't be told apart, and blocking blocks the origin ⚠️ (documented in docs/API.md) | Low |
| S12 | Stolen or leaked bearer token | 256-bit random, SHA-256 at rest, recognizable prefixes (`mau_pat_`, `mau_at_`, `mau_rt_`) for secret scanning, shown once; access tokens last an hour; refresh tokens work once and a reused one ends the grant; anyone can revoke a found token at `/oauth/revoke`; listed with last use in Settings ✅. Personal tokens may never expire (user's choice, 2026-11-25) and survive a Settings password change (decision) ⚠️ | Medium |
| S13 | Intercepted authorization code | PKCE S256 required for every app; codes last 60 seconds, bound to app, return address and challenge, and work once (a reused code ends the grant); `iss` returned against mix-ups between instances; `response_mode=fragment` keeps codes out of a static host's logs ✅ | Low |

### Tampering

| ID | Threat | Controls | Risk |
|---|---|---|---|
| T1 | Cross-site request forgery | SameSite=Lax cookie, plus an exact `Origin` check on every non-GET API request ✅ | Low |
| T2 | Changing another user's records through sync | Every operation checks that the book, reading, copy, or private work belongs to the session's user; each runs in a savepoint ✅ | Low |
| T3 | Malformed sync data (huge IDs, path-like cover names) | IDs of new records ≤ 64 characters `[A-Za-z0-9_-]`; cover overrides must be stored 32-hex hashes; field lengths, dates, pages, ranks, and ratings validated ✅ | Low |
| T4 | Poisoning the shared catalog | Users can add works only from cached provider results; manual entries stay private until an admin promotes them; admin edits are audited ✅. **A mau export only names a provider record (well-formed IDs only); the work is looked up at the provider and added as the provider describes it, otherwise the row is matched like any other file and at worst becomes a private entry (finding V2-1, fixed)** ✅ | Low |
| T5 | Stored XSS through titles, notes, bios | Svelte and maud escape all text; no `{@html}` or `innerHTML`; strict CSP (scripts from self, plus the shell's inline script by hash) as a second layer ✅ | Low |
| T6 | Clickjacking | `X-Frame-Options: DENY` and `frame-ancestors 'none'` ✅ | Low |
| T7 | Tampering with files on disk by other local users | Data directory forced to mode 700 on every start ✅; **on maubooks.org the other services (Forgejo as `git`, its runner as `runner`, Caddy as `caddy`) are separate users, and the runner's jobs run in containers** ✅ | Low |
| **T8** | **Malicious code reaching `main` (a stolen GitHub session or token, or a mistake)** | **CI must pass first; the `production` secrets reach only jobs on `main`; pull requests from forks run without secrets, and their caches can't be read by `main` ✅. A push runs as `mau` within minutes, with no review step ⚠️ (the chosen trade-off for deploying on every push). GitHub two-factor and branch protection against force-pushes are the operator's 🔭** | Medium |
| **T9** | **Compromised build dependency or GitHub action** | **Lock files committed and used (`npm ci`, `--locked`); third-party actions pinned to commits; Rust pinned to 1.96.0; `cargo audit` and `npm audit --omit=dev` on every run, and a weekly run of `main` catches advisories published between pushes ✅** | Medium |
| T10 | Cross-site requests riding the reader's session to allow an app or make a token | Both are cookie-API POSTs: SameSite=Lax, the exact `Origin` check, a recent password confirmation, and `X-Frame-Options: DENY` / `frame-ancestors 'none'` against clickjacking the consent screen ✅ | Low |
| T11 | Open redirect through `/oauth/authorize` or sign-in's `next` | Error redirects go only to an already-validated return address; `next` accepts only a path on this site (`//host`, `/\host`, schemes go Home) ✅ | Low |
| T12 | Writing beyond a token's scopes, or to another reader's rows, through `/api/v1` | Scopes checked per endpoint and per field (a `PATCH` with `notes` needs `notes:write`); writes are sync operations checked for ownership by `library::apply`; other readers' rows answer 404 (tested for every book endpoint) ✅ | Low |
| T13 | An app adding made-up works to the shared catalog | `POST /api/v1/books` with `catalog` uses only cached provider results, like the app; `manual` works stay private ✅ (same as T4) | Low |

### Repudiation

| ID | Threat | Controls | Risk |
|---|---|---|---|
| R1 | An admin denies an action | Audit log of admin and account-level actions, kept forever ✅. The log lives in the same database an admin can't edit through the app, but the operator can ⚠️ | Low |
| R2 | A user doesn't notice account misuse | Recovery activity lists recovery-code use, reset links, and code lockouts; Sessions lists devices and last use ✅. No email or push notifications ⚠️ | Medium |
| **R3** | **Not knowing what was deployed when** | **Each deploy is logged with its commit (`journalctl -t mau-deploy`), and GitHub keeps the run, its tests, and its artifact for 30 days ✅** | Low |
| R4 | A reader not noticing what has access | Settings lists every connected app and personal token with what it may do and when it was last used, with Disconnect and Revoke ✅. The audit log records only admins blocking apps, not who connected what (deliberate, see I13); no notification when a token is made ⚠️ | Low |

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
| **I9** | **Server log lines in public CI logs** | **What `mau-deploy` prints over SSH ends up in the public Actions log, so it prints only its own steps; mau's log (which on an instance without an admin holds the setup link) stays in the journal (finding V2-3, fixed)** ✅ | Low |
| **I10** | **Deployment details in the public repository** | **`deploy/` and the workflow name the tailnet address, host key, paths, and users. Reaching any of them needs the tailnet and the deploy key ⚠️** | Low |
| I11 | Reading data beyond what was allowed | `rating`, `notes`, `copies` and the current reading appear only with their read scopes; notes are marked Private on the consent screen; no scope reaches the account, the Google Books key, sessions or exports ✅ | Low |
| I12 | Cross-origin reads of cookie-authenticated data | The cookie API sends no CORS headers; the CORS-open endpoints never read cookies; `Access-Control-Allow-Credentials` is never sent ✅ (tested) | Low |
| I13 | Admins learning which apps a reader uses | Admin › Apps shows each app's number of people, never who; the audit log doesn't record connections or tokens ✅ (finding V3-4, fixed) | Low |
| I14 | Tokens in a static app's browser storage | The app's responsibility; refresh rotation with reuse detection limits a stolen copy ⚠️ (docs/API.md says to keep only the newest) | Low |

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
| D8 | Filling server memory with authorization requests | Requests last 10 minutes, at most 10,000 (oldest dropped), 60 a minute per address ✅. A flood from many addresses can push out a reader's pending request; they start again ⚠️ | Low |
| D9 | An app piling up token rows | At most 20 live access tokens and 50 spent refresh tokens per reader and app; expired tokens deleted hourly; 50 personal tokens per reader ✅ (finding V3-3, fixed) | Low |
| D10 | Using `/api/v1` to load the server or provider quotas | 120 requests a minute per token; catalog search shares the reader's 30 a minute; adding catalog books needs a cached search result ✅ | Low |

### Elevation of privilege

| ID | Threat | Controls | Risk |
|---|---|---|---|
| E1 | Non-admin reaching admin endpoints | `Admin` extractor on every admin handler; invites limited to the creator's own unless admin ✅ | Low |
| E2 | Removing the last admin | Refused for demote, suspend, and delete ✅ | Low |
| E3 | Sensitive changes from a stolen session | Password, username, two-factor, recovery codes, and account deletion need a password (and code) confirmation within 10 minutes ✅; **so do an admin's reset links for others, deleting users, and granting admin** ✅. The Google Books key does not (decision 2026-11-08) ⚠️ | Low |
| E4 | Server-side request forgery through cover URLs | https only; host allowlist of provider image hosts; redirects only to https domain names (no IP literals or localhost), at most five; responses read with a 10 MB cap ✅. **A mau export's provider IDs must look like Open Library or Google IDs before they're used in a URL (finding V2-4, fixed)** ✅ | Low |
| E5 | Supply-chain compromise of dependencies | See T9: **`cargo audit` and `npm audit --omit=dev` run in CI on every push and weekly** ✅; npm has 4 low dev-only advisories, all one Dependabot alert (GHSA-gcqx-w48m-w2h4, deepmerge, used by `@sveltejs/vite-plugin-svelte` at build time on mau's own config; none at runtime; no fixed version yet), left open by decision 2026-11-12 ⚠️ | Medium |
| **E6** | **The deploy path giving more than `mau`'s rights** | **Nothing uploaded runs as root: the binary runs as `mau` under systemd hardening, and `mau-deploy` runs both its `--version` check and the database copy as `mau` (finding V2-2, fixed) ✅. The script itself runs as root, and takes only a checksum, a commit hash, and the binary** | Low |
| **E7** | **Another service on the shared host reaching mau** | **Separate users, mau's data directory 700, systemd's `ProtectSystem=strict`/`ProtectHome`, Forgejo registration closed, runner jobs in containers ✅. A local privilege escalation in any of them reaches everything ⚠️. Forgejo listens on all addresses (shielded only by the Hetzner firewall) and sshd allows passwords for non-root users 🔭** | Low |
| E8 | A token reaching admin powers, account settings, or making more tokens | `/api/v1` is a separate router; the cookie API's `Authed` and `Admin` extractors read only the session cookie; the scope list has nothing for the account, tokens, imports, invites or admin; tested by calling every cookie and admin endpoint with an admin's all-scopes token (all 401/403) ✅ | Low |
| E9 | A token passing a password confirmation | Handlers reused for the API run as a session that has never confirmed (`reauth_at` empty), so anything needing a confirmation refuses ✅ | Low |
| E10 | Server-side request forgery through an app's metadata address | `client_id` must be an https domain name (no IP literals, no `*.localhost`); local addresses are never fetched; fetches use a resolver that returns public addresses only (no loopback, private, link-local, CGNAT, NAT64/6to4-embedded or reserved ranges), no proxies, no redirects, https on port 443 only, 5 seconds, 16 KB ✅ (finding V3-2, fixed) | Low |
| E11 | A blocked or suspended account's app keeping access | Blocking deletes the address's connections and is checked at authorize, approve and token time; suspended accounts' tokens are refused and their refreshes fail; recovery and reset links end every token ✅ (tested) | Low |

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

The 2026-11-11 review's findings V2-1 to V2-5 are fixed (Appendix A).

1. Upgrade SQLite once sqlx accepts `libsqlite3-sys` ≥ 0.39 (I8).
2. For the operator: an off-server backup copy (D6); GitHub two-factor and
   branch protection against force-pushes and deletion (T8); confirm the
   tailnet policy (S9); bind Forgejo to localhost and turn off SSH password
   logins (E7).
3. Optional proxy-level rate limiting in Caddy (D3).
4. Consider encrypting TOTP secrets and API keys with a key kept outside the
   database, if backups are stored where others can read them (I3).
5. Queue a server sign-out when signing out offline (I5).
6. Consider a notice to the reader when a personal token is made or an app
   is connected (R4), and an optional maximum lifetime for personal tokens
   that an admin can set (S12).

## Appendix C: review of 2026-11-25 (API access)

Reviewed: everything added for API access: `api/token.rs` (lookup,
extractor, revocation), `api/scope.rs`, `api/client.rs` (client IDs, redirect
rules, metadata fetching and its resolver), `api/oauth.rs` (authorize,
consent, token, revoke, discovery), `api/v1.rs`, `api/manage.rs`, the router
and CORS layers, the changes to password reset, the sign-in `next`
parameter, the consent page, Settings › Access Tokens, Admin › Apps and
the service worker. Checks: 226 server tests (27 new for API access,
including every cookie and admin endpoint called with an admin's token,
cross-user access on every book endpoint, code and refresh-token reuse,
blocking, suspension and reset links), 246 web tests, `cargo clippy`, and a
headless Firefox run of the whole flow (consent, allow, return with a code,
making a token, blocking) at 390 and 1440 px in light and dark, with axe
(no violations) and no console errors.

| # | Finding | Severity | Outcome |
|---|---|---|---|
| V3-1 | A blocked app's metadata was fetched before the block was checked | Low | Fixed: the block is checked first |
| V3-2 | Metadata fetches went to any port of a public host (probing other services from the server) | Low | Fixed: port 443 only; private addresses were already refused |
| V3-3 | An app refreshing often piled up spent refresh tokens and live access tokens without limit (D9) | Low | Fixed: caps per reader and app, hourly cleanup of expired tokens |
| V3-4 | The audit log named who connected which app and made which token, while Admin › Apps promised a count only (I13) | Low | Fixed: only blocking is recorded |
| V3-5 | Allowing an app again when its metadata couldn't be read erased its name | Bug | Fixed |
| V3-6 | A metadata document that failed once was treated as missing for a day | Bug | Fixed: retried after 5 minutes |
| V3-7 | mau's OAuth error page overflowed phone screens, lacked the site footer and font, and broke "https://" across lines | Bug | Fixed |
| V3-8 | Admin › Apps ran two phrases together and said "Blocked" twice | Bug | Fixed |
| V3-9 | `change_password`'s comment contradicted the code (sessions are signed out) | Doc | Fixed |
| V3-10 | Apps on one origin can't be told apart (S11) | Low | Accepted: identity is the origin |
| V3-11 | Personal tokens may never expire and survive a Settings password change (S12) | Medium | Accepted (user's choice and decision); §8.6 |
| V3-12 | Signing up from the consent screen loses the way back | UX | Fixed: sign-up keeps `next` like sign-in |

Follow-up the same day: the API pages joined the Firefox suite (`api`: 10
steps, a static app on a second origin signing in for real), and 9 more API
tests check that every `/api/v1` route refuses a token missing its scope,
plus each endpoint's own behavior (catalog search and adding a result,
shares, edits, copies, percent readings, expired tokens, the token endpoint's
errors, the token limit). 235 server tests in all.

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
| V2-1 | A crafted mau export can add made-up works to the shared catalog (T4) | Medium | Fixed: works come from the provider's answer; unconfirmed rows are matched normally |
| V2-2 | The deploy script runs the uploaded binary as root to check it (E6) | Medium | Fixed: checked as `mau` |
| V2-3 | A failed deploy copies server log lines, possibly a setup link, into public CI logs (I9) | Medium | Fixed: the log stays on the server |
| V2-4 | Provider IDs from an export used in URLs unchecked (E4) | Low | Fixed: validated first |
| V2-5 | Advisories between pushes go unnoticed; `npm audit` not automated (T9) | Low | Fixed: weekly CI run; `npm audit --omit=dev` in CI |
| V2-6 | Backups only on the same volume as the database (D6) | Medium | Accepted by the operator |
| V2-7 | Forgejo on all addresses; sshd password logins on (E7) | Low | Operator (§8.2) |
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
