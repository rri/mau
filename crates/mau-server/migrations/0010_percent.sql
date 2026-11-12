-- Readings tracked by percent, for a copy whose page count isn't known or
-- isn't wanted: `unit` is 'percent', `total` is 100, and `pages` keeps the
-- copy's page count if the reader entered one (only used in percent mode).
-- Each progress event records the unit it moved in, so "pages read" counts
-- page events only.
ALTER TABLE readings ADD COLUMN unit TEXT NOT NULL DEFAULT 'pages' CHECK (unit IN ('pages', 'percent'));
ALTER TABLE readings ADD COLUMN pages INTEGER;
ALTER TABLE progress_events ADD COLUMN unit TEXT NOT NULL DEFAULT 'pages' CHECK (unit IN ('pages', 'percent'));
