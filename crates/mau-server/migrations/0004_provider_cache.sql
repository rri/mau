-- Catalog provider responses, kept for 30 days to save requests and quota.
CREATE TABLE provider_cache (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
) STRICT;
