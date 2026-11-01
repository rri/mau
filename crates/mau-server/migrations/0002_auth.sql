-- Accounts, passkeys, sessions, and recovery. Times are Unix milliseconds.

CREATE TABLE users (
    id           TEXT PRIMARY KEY NOT NULL,
    username     TEXT NOT NULL UNIQUE,          -- lowercase
    display_name TEXT NOT NULL,
    bio          TEXT NOT NULL DEFAULT '',
    time_zone    TEXT NOT NULL,                 -- IANA name, e.g. Europe/Berlin
    is_admin     INTEGER NOT NULL DEFAULT 0,
    suspended    INTEGER NOT NULL DEFAULT 0,
    webauthn_id  TEXT NOT NULL UNIQUE,          -- UUID given to authenticators
    created_at   INTEGER NOT NULL
) STRICT;

-- Old usernames stay reserved for their previous owner for 30 days.
CREATE TABLE username_reservations (
    username TEXT PRIMARY KEY NOT NULL,
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    until    INTEGER NOT NULL
) STRICT;

CREATE TABLE passkeys (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    cred_id      TEXT NOT NULL UNIQUE,          -- base64url credential ID
    data         TEXT NOT NULL,                 -- webauthn-rs Passkey as JSON
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
) STRICT;
CREATE INDEX passkeys_user ON passkeys(user_id);

CREATE TABLE sessions (
    id           TEXT PRIMARY KEY NOT NULL,     -- public ID for listing/revoking
    token_hash   TEXT NOT NULL UNIQUE,          -- SHA-256 of the cookie value
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device       TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    restricted   INTEGER NOT NULL DEFAULT 0,    -- 1: may only add a passkey
    reauth_at    INTEGER                        -- last passkey confirmation
) STRICT;
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE recovery_codes (
    id      INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    hash    TEXT NOT NULL,                      -- Argon2id PHC string
    used_at INTEGER
) STRICT;
CREATE INDEX recovery_codes_user ON recovery_codes(user_id);

-- Shown to the user under "Recovery activity".
CREATE TABLE recovery_events (
    id      INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    at      INTEGER NOT NULL,
    message TEXT NOT NULL
) STRICT;

CREATE TABLE invites (
    id         TEXT PRIMARY KEY NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER,
    used_by    TEXT REFERENCES users(id) ON DELETE SET NULL,
    revoked    INTEGER NOT NULL DEFAULT 0
) STRICT;

-- One-time links an admin issues so a locked-out user can add a passkey.
CREATE TABLE enroll_links (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER
) STRICT;

CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY,
    at         INTEGER NOT NULL,
    actor_id   TEXT,
    actor_name TEXT NOT NULL,
    action     TEXT NOT NULL
) STRICT;
