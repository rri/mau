-- The catalog (shared works and editions, plus users' private manual entries)
-- and each user's library, which syncs to their devices.
--
-- Synced rows carry:
--   seq     the global change counter at their last change (devices pull
--           rows with seq greater than the last one they saw)
--   clock   JSON object of per-field HLC timestamps (last writer wins)
--   deleted tombstone flag; operations on deleted rows are ignored

CREATE TABLE sync_seq (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    n  INTEGER NOT NULL
) STRICT;
INSERT INTO sync_seq (id, n) VALUES (1, 0);

-- ---- Catalog ---------------------------------------------------------------

CREATE TABLE works (
    id          TEXT PRIMARY KEY NOT NULL,
    title       TEXT NOT NULL,
    first_year  INTEGER,
    cover       TEXT,                      -- cover hash, see covers.rs
    owner_id    TEXT REFERENCES users(id) ON DELETE CASCADE, -- NULL: shared
    suggested   INTEGER NOT NULL DEFAULT 0, -- owner asked for it to be shared
    source      TEXT,                      -- provider name, e.g. openlibrary
    source_id   TEXT,                      -- provider's ID for the work
    created_at  INTEGER NOT NULL,
    seq         INTEGER NOT NULL
) STRICT;
CREATE INDEX works_source ON works(source, source_id);
CREATE INDEX works_owner ON works(owner_id);

CREATE TABLE work_people (
    work_id  TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    name     TEXT NOT NULL,
    role     TEXT NOT NULL CHECK (role IN ('author', 'translator', 'editor', 'illustrator')),
    PRIMARY KEY (work_id, position)
) STRICT;

CREATE TABLE editions (
    id         TEXT PRIMARY KEY NOT NULL,
    work_id    TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    publisher  TEXT,
    binding    TEXT,                       -- Hardcover, Paperback, Ebook, Audiobook
    year       INTEGER,
    isbn13     TEXT,
    pages      INTEGER,                    -- catalog's page count (a hint only)
    cover      TEXT,
    owner_id   TEXT REFERENCES users(id) ON DELETE CASCADE,
    source     TEXT,
    source_id  TEXT,
    seq        INTEGER NOT NULL
) STRICT;
CREATE INDEX editions_work ON editions(work_id);
CREATE INDEX editions_isbn ON editions(isbn13);

-- ---- Library (synced) ------------------------------------------------------

CREATE TABLE books (
    id               TEXT PRIMARY KEY NOT NULL,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    work_id          TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    rank             TEXT NOT NULL,         -- manual order, see rank.rs
    active           INTEGER NOT NULL DEFAULT 0,
    rating           INTEGER NOT NULL DEFAULT 0,  -- 0 = none, else 1-5
    notes            TEXT NOT NULL DEFAULT '',
    title_override   TEXT,
    authors_override TEXT,                  -- JSON array of names
    cover_override   TEXT,
    added_at         INTEGER NOT NULL,
    deleted          INTEGER NOT NULL DEFAULT 0,
    clock            TEXT NOT NULL DEFAULT '{}',
    seq              INTEGER NOT NULL
) STRICT;
CREATE INDEX books_user_seq ON books(user_id, seq);
CREATE INDEX books_work ON books(work_id);

-- Tags are a last-writer-wins set: one row per (book, tag), present or not.
CREATE TABLE book_tags (
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    present INTEGER NOT NULL,
    clock   TEXT NOT NULL,
    seq     INTEGER NOT NULL,
    PRIMARY KEY (book_id, tag)
) STRICT;
CREATE INDEX book_tags_user_seq ON book_tags(user_id, seq);

CREATE TABLE copies (
    id         TEXT PRIMARY KEY NOT NULL,
    book_id    TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    format     TEXT NOT NULL,
    location   TEXT NOT NULL DEFAULT '',
    edition_id TEXT REFERENCES editions(id) ON DELETE SET NULL,
    deleted    INTEGER NOT NULL DEFAULT 0,
    clock      TEXT NOT NULL DEFAULT '{}',
    seq        INTEGER NOT NULL
) STRICT;
CREATE INDEX copies_user_seq ON copies(user_id, seq);

CREATE TABLE readings (
    id          TEXT PRIMARY KEY NOT NULL,
    book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_on  TEXT NOT NULL,              -- local date, YYYY-MM-DD
    finished_on TEXT,
    total       INTEGER NOT NULL,
    pos         INTEGER NOT NULL DEFAULT 0,
    pos_clock   TEXT NOT NULL DEFAULT '',   -- HLC of the current position
    deleted     INTEGER NOT NULL DEFAULT 0,
    clock       TEXT NOT NULL DEFAULT '{}',
    seq         INTEGER NOT NULL
) STRICT;
CREATE INDEX readings_user_seq ON readings(user_id, seq);
CREATE INDEX readings_book ON readings(book_id);

-- Every change of position. Events on a branch that lost a conflict are
-- marked deleted so they don't count as reading.
CREATE TABLE progress_events (
    id         TEXT PRIMARY KEY NOT NULL,
    reading_id TEXT NOT NULL REFERENCES readings(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    hlc        TEXT NOT NULL,
    local_date TEXT NOT NULL,
    from_page  INTEGER NOT NULL,
    to_page    INTEGER NOT NULL,
    deleted    INTEGER NOT NULL DEFAULT 0,
    seq        INTEGER NOT NULL
) STRICT;
CREATE INDEX progress_events_user_seq ON progress_events(user_id, seq);
CREATE INDEX progress_events_reading ON progress_events(reading_id, hlc);
