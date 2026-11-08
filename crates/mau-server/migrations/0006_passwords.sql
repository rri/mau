-- no-transaction
-- Passwords with optional authenticator-app codes replace passkeys.
--
-- Rebuilding `users` (to drop the unique `webauthn_id` column) needs foreign
-- key enforcement off, which SQLite only allows outside a transaction; the
-- changes themselves run in one.

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE users_new (
    id             TEXT PRIMARY KEY NOT NULL,
    username       TEXT NOT NULL UNIQUE,        -- lowercase
    display_name   TEXT NOT NULL,
    bio            TEXT NOT NULL DEFAULT '',
    time_zone      TEXT NOT NULL,               -- IANA name, e.g. Europe/Berlin
    is_admin       INTEGER NOT NULL DEFAULT 0,
    suspended      INTEGER NOT NULL DEFAULT 0,
    password_hash  TEXT,                        -- Argon2id PHC string; NULL: needs a reset link
    totp_secret    TEXT,                        -- base32; NULL: two-factor is off
    totp_last_step INTEGER,                     -- last accepted 30-second step, against replays
    created_at     INTEGER NOT NULL
) STRICT;

INSERT INTO users_new (id, username, display_name, bio, time_zone, is_admin, suspended, created_at)
    SELECT id, username, display_name, bio, time_zone, is_admin, suspended, created_at FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

DROP TABLE passkeys;
DROP TABLE enroll_links;

-- Sessions signed in with a recovery code were limited to adding a passkey.
-- Recovery now sets a new password, so there are no limited sessions.
DELETE FROM sessions WHERE restricted = 1;
ALTER TABLE sessions DROP COLUMN restricted;

-- One-time links an admin (or `mau reset-password`) issues so a locked-out
-- user can set a new password.
CREATE TABLE reset_links (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER
) STRICT;

COMMIT;

PRAGMA foreign_keys = ON;
