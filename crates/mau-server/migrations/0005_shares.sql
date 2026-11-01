-- Published (public) parts of a library. Each has its own link token and is
-- listed on the owner's public profile.
CREATE TABLE shares (
    id              TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('finished', 'active', 'tag', 'catalog')),
    tag             TEXT,
    include_ratings INTEGER NOT NULL DEFAULT 0,
    token           TEXT NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL
) STRICT;
CREATE INDEX shares_user ON shares(user_id);
