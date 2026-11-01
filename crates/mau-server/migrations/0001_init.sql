-- Key-value store for instance-wide settings that admins change at runtime
-- (signup mode, provider order, ...). Values are JSON.
CREATE TABLE instance_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
