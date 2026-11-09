# mau threat model

Version 1, 2026-11-09. Method: STRIDE per element of the data-flow diagram,
with risks rated by the OWASP Risk Rating Methodology (likelihood × impact,
each Low / Medium / High). Controls are cross-referenced to OWASP ASVS 4.0
chapters where it helps. Review this document when a trust boundary, an entry
point, or the storage of a secret changes, and at least once a year.

## 1. System overview

mau is a self-hosted personal library and reading tracker: one Rust binary
(Axum, SQLite) serving a SvelteKit single-page app, a JSON API under `/api`,
server-rendered public pages (`/u/{name}`, `/s/{token}`), and cover images
(`/covers/{hash}-{size}.jpg`). It is meant to run behind the operator's own
HTTPS reverse proxy for a small group: an individual, a family, a book club.

Out of scope: the reverse proxy, TLS termination, the host OS, and the
operator's backup storage. These are the operator's (see §7).

## 2. Assets

| ID | Asset | Why it matters |
|---|---|---|
| A1 | Account credentials: password hashes (Argon2id), recovery-code hashes, TOTP secrets (plain), session-token hashes, reset-link and invite-token hashes | Account takeover |
| A2 | Library data: books, reading history, notes, ratings, tags, copies and their locations | Private by default; notes are never shared |
| A3 | Third-party API keys: the admin's provider keys and users' own Google Books keys (plain) | Quota theft, billing |
| A4 | Admin powers: user management, signup mode, shared catalog, audit log | Instance takeover |
| A5 | The shared catalog (works, editions, covers) | Integrity for everyone on the instance |
| A6 | Availability of the instance, and its standing with Open Library and Google | A small instance can be blocked by providers |
| A7 | Backups in `data_dir/backups` (copies of A1 to A5) | Same as the live database |

## 3. Actors

| Actor | Trust | Capabilities |
|---|---|---|
| Anonymous internet user | None | Public pages, sign-in, signup when open or invited, setup and reset links if they hold one |
| Signed-in user | Own data only | Everything under their account; reads the shared catalog; adds works to it via provider results; suggests manual entries |
| Admin | High, within the app | Users, signup, providers, shared catalog, audit log, reset links for others |
| Operator (server shell) | Full | Config, data directory, `mau reset-password`, `mau backup` |
| Catalog providers (Open Library, Google Books, Hardcover, ISBNdb) | Partial: data is used, never executed | Supply titles, people, editions, cover URLs |
| Other local accounts on the host | None | Whatever file permissions allow |

## 4. Data flow and trust boundaries

```
                    TB1: internet / HTTPS proxy
 Browser (SPA, IndexedDB, ────────────────┐
 service worker)                          │
   │  JSON /api, cookie mau_session       ▼
   │                        ┌──────────────────────────────┐   TB3: outbound
   ├──── GET /u, /s ───────►│ mau (Axum)                   │──────────────────►  Providers
   │                        │  client_ip → rate limits →   │   https only,         (search, editions,
   │                        │  origin check → handlers     │   cover host           covers)
   └──── GET /covers ──────►│  security headers (CSP…)     │   allowlist
                            └──────────────┬───────────────┘
                     TB2: process / disk   │
                            ┌──────────────▼───────────────┐
                            │ data_dir (700): mau.db, -wal, │
                            │ covers/, backups/             │
                            └───────────────────────────────┘
 Operator shell ── mau backup / reset-password / config file (TB4: host)
```

- **TB1** browser ↔ server: everything a client sends is untrusted.
- **TB2** server ↔ disk: only the mau account may read the data directory.
- **TB3** server → providers: responses are untrusted input; requests must
  not be steerable to internal hosts.
- **TB4** operator ↔ host: trusted; the setup link and reset links are
  printed only to the server's own log or terminal.

## 5. Entry points

| Entry point | Auth | Notes |
|---|---|---|
| `POST /api/auth/*` (login, code, register, recovery, reset, confirm) | None or ticket | Rate-limited per IP and per account |
| `POST /api/sync` | Session | Up to 500 operations; each is checked for ownership |
| `/api/catalog/*` | Session | Search calls providers (30 a minute per user) |
| `/api/import*` | Session | Bodies up to 30 MB, at most 5,000 rows, one import at a time |
| `POST /api/covers`, admin cover upload | Session / admin | Bodies up to 10 MB; decoded with caps |
| `/api/admin/*` | Admin | |
| `GET /u/{name}`, `/s/{token}` | None | Server-rendered, escaped by maud |
| `GET /covers/{name}` | None | Names are content hashes; strict pattern |
| Config file and environment | Operator | `public_url` must be https (or localhost) |

## 6. Threats and controls (STRIDE)

Risk is the rating *after* the listed controls. Status: ✅ in place,
⚠️ accepted residual risk, 🔭 recommended for later.

### Spoofing

| ID | Threat | Controls | Risk |
|---|---|---|---|
| S1 | Password guessing and credential stuffing | Argon2id (crate defaults: 19 MiB, t=2); minimum 10 characters plus a blocklist of common passwords; 20 attempts a minute per IP and 10 per username; same answer and timing for unknown accounts (dummy hash) ✅ | Low |
| S2 | Guessing authenticator codes after a stolen password | Codes used once (last step stored); 5 tries per sign-in; **10 wrong codes per account within an hour block codes for 15 minutes and leave a warning in the account's recovery activity** ✅ | Low |
| S3 | Guessing recovery codes | ~49 bits each, Argon2-hashed, single use; 5 attempts a minute per IP and per username ✅ | Low |
| S4 | Stolen session cookie | 256-bit random token, only its SHA-256 stored; HttpOnly, SameSite=Lax, Secure on https; 30-day sliding expiry; revocable per device; password change and reset end other sessions ✅. No IP or device binding ⚠️ | Low |
| S5 | Setup-link takeover of a fresh instance | Link printed only to the server log; valid until the first admin exists; new link each start ✅ | Low |
| S6 | Leaked reset or invite links | Hashed at rest; reset links last 24 hours, single use, only the newest works, and turn two-factor off (shown on the page); invites expire and are single use ✅ | Low |
| S7 | Spoofed client IP to dodge rate limits | `X-Forwarded-For` honored only from `trusted_proxies`, right-most untrusted entry ✅ | Low |

### Tampering

| ID | Threat | Controls | Risk |
|---|---|---|---|
| T1 | Cross-site request forgery | SameSite=Lax cookie, plus an exact `Origin` check on every non-GET API request ✅ | Low |
| T2 | Changing another user's records through sync | Every operation checks that the book, reading, copy, or private work belongs to the session's user; each runs in a savepoint ✅ | Low |
| T3 | Malformed sync data (huge IDs, path-like cover names) | IDs of new records ≤ 64 characters `[A-Za-z0-9_-]`; cover overrides must be stored 32-hex hashes; field lengths, dates, pages, ranks, and ratings validated ✅ | Low |
| T4 | Poisoning the shared catalog | Users can add works only from cached provider results; manual entries stay private until an admin promotes them; admin edits are audited ✅ | Low |
| T5 | Stored XSS through titles, notes, bios | Svelte and maud escape all text; no `{@html}` or `innerHTML`; strict CSP (scripts from self, plus the shell's inline script by hash) as a second layer ✅ | Low |
| T6 | Clickjacking | `X-Frame-Options: DENY` and `frame-ancestors 'none'` ✅ | Low |
| T7 | Tampering with files on disk by other local users | Data directory forced to mode 700 on every start ✅ | Low |

### Repudiation

| ID | Threat | Controls | Risk |
|---|---|---|---|
| R1 | An admin denies an action | Audit log of admin and account-level actions, kept forever ✅. The log lives in the same database an admin can't edit through the app, but the operator can ⚠️ | Low |
| R2 | A user doesn't notice account misuse | Recovery activity lists recovery-code use, reset links, and code lockouts; Sessions lists devices and last use ✅. No email or push notifications ⚠️ | Medium |

### Information disclosure

| ID | Threat | Controls | Risk |
|---|---|---|---|
| I1 | Reading another user's library | All reads scoped to the session's user; private works visible only to their owner ✅ | Low |
| I2 | Account enumeration | Sign-in gives one answer for unknown or wrong; **profiles with nothing published look like unknown names** ✅. Username availability check and signup reveal taken names (needed for signup; 60 a minute per IP) ⚠️ | Low |
| I3 | Secrets readable on disk | Directory mode 700 ✅. TOTP secrets and API keys stored in plain text (needed to use them) ⚠️; full-disk or volume encryption is the operator's 🔭 | Medium |
| I4 | Backups leaking | Kept inside the private data directory ✅; copies elsewhere are the operator's responsibility ⚠️ | Medium |
| I5 | Data left on a shared browser | Sign-out deletes the device's IndexedDB copy ✅. Signing out while offline leaves the server session alive until it expires ⚠️ | Low |
| I6 | Tracking through third-party requests | Covers are downloaded by the server and served from mau; the browser never contacts providers; `Referrer-Policy: same-origin` ✅ | Low |
| I7 | Public pages exposing too much | Only published items; notes never; ratings only if chosen ✅ | Low |
| I8 | SQLite stale-memory bug (RUSTSEC-2026-0459, CVE-2026-119375) in the bundled SQLite | Needs a crafted `.db` + `-wal` pair placed where mau opens it; blocked by the private data directory ⚠️. Upgrade when sqlx allows `libsqlite3-sys` ≥ 0.39 🔭 | Low |

### Denial of service

| ID | Threat | Controls | Risk |
|---|---|---|---|
| D1 | Decompression-bomb images | Uploads ≤ 10 MB; decoding capped at 10,000 px a side and 256 MB, two at a time ✅ | Low |
| D2 | Runaway imports exhausting memory or provider goodwill | 30 MB bodies, 5,000 rows, 5 starts a minute, **one import in progress per user**, 3 provider requests at a time, 30-day provider cache ✅ | Low |
| D3 | Argon2 CPU exhaustion via sign-in endpoints | Per-IP limits ✅; a distributed attack could still load the CPU ⚠️ (proxy-level limits 🔭) | Medium |
| D4 | Locking someone out by failing their sign-ins | Per-username limit blocks password attempts for a minute; code lockout lasts 15 minutes and needs the right password first ✅ | Low |
| D5 | Large sync batches | 500 operations per request, default 2 MB request bodies elsewhere ✅ | Low |

### Elevation of privilege

| ID | Threat | Controls | Risk |
|---|---|---|---|
| E1 | Non-admin reaching admin endpoints | `Admin` extractor on every admin handler; invites limited to the creator's own unless admin ✅ | Low |
| E2 | Removing the last admin | Refused for demote, suspend, and delete ✅ | Low |
| E3 | Sensitive changes from a stolen session | Password, username, two-factor, recovery codes, and account deletion need a password (and code) confirmation within 10 minutes ✅. The Google Books key does not (decision 2026-11-08) ⚠️ | Low |
| E4 | Server-side request forgery through cover URLs | https only; host allowlist of provider image hosts; redirects only to https domain names (no IP literals or localhost), at most five; responses read with a 10 MB cap ✅ | Low |
| E5 | Supply-chain compromise of dependencies | Lock files committed; `cargo audit` and `npm audit` run in the audit (one dev-only npm advisory, deepmerge, unused at runtime) ✅; automated scanning in CI 🔭 | Medium |

## 7. Assumptions and operator responsibilities

- HTTPS terminates at a proxy the operator controls; `public_url` is https.
  mau sends HSTS (one year, no `includeSubDomains`, no preload).
- The proxy passes the real client address in `X-Forwarded-For`, and its
  address is in `trusted_proxies`.
- The host is single-purpose or other local accounts are untrusted; the
  service runs as its own `mau` user with the hardening in README.
- Backups copied off the host are encrypted by the operator.
- Admins are trusted with everyone's account recovery (reset links) but not
  with reading private libraries through the app; the operator can read all.

## 8. Recommendations not yet done

1. Upgrade SQLite once sqlx accepts `libsqlite3-sys` ≥ 0.39 (I8).
2. Run `cargo audit` and `npm audit` automatically (E5).
3. Optional proxy-level rate limiting for public deployments (D3).
4. Consider encrypting TOTP secrets and API keys with a key kept outside the
   database, if backups are stored where others can read them (I3).
5. Queue a server sign-out when signing out offline (I5).

## Appendix: audit of 2026-11-09

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
