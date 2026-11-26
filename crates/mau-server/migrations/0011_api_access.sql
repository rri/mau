-- API access: personal tokens, and apps people connect through OAuth.
-- Times are Unix milliseconds.

-- Apps someone has connected. An app is known by its address (the OAuth
-- client_id); nothing is registered ahead of time. A row is added the first
-- time someone allows the app, so links nobody accepted leave nothing behind.
CREATE TABLE oauth_clients (
    id         TEXT PRIMARY KEY NOT NULL,   -- client_id: the app's https:// address
    origin     TEXT NOT NULL,               -- scheme://host[:port], what people are shown
    name       TEXT,                        -- from the app's metadata document, if any
    blocked    INTEGER NOT NULL DEFAULT 0,  -- an admin blocked this address
    first_seen INTEGER NOT NULL
) STRICT;
CREATE INDEX oauth_clients_origin ON oauth_clients(origin);

-- A person's permission for an app: "Connected apps" in Settings.
CREATE TABLE oauth_grants (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id    TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    scopes       TEXT NOT NULL,             -- space-separated, everything ever allowed
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER,
    UNIQUE (user_id, client_id)
) STRICT;
CREATE INDEX oauth_grants_client ON oauth_grants(client_id);

-- Bearer tokens. Only the SHA-256 of each is stored.
CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,             -- personal | access | refresh
    grant_id     TEXT REFERENCES oauth_grants(id) ON DELETE CASCADE,
    name         TEXT NOT NULL DEFAULT '',  -- personal tokens
    token_hash   TEXT NOT NULL UNIQUE,
    hint         TEXT NOT NULL,             -- the token's first characters, e.g. mau_pat_3fA9
    scopes       TEXT NOT NULL,             -- space-separated
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER,                   -- NULL: never (personal tokens only)
    last_used_at INTEGER,
    used_at      INTEGER                    -- refresh tokens: exchanged (kept to spot reuse)
) STRICT;
CREATE INDEX api_tokens_user ON api_tokens(user_id);
CREATE INDEX api_tokens_grant ON api_tokens(grant_id);
