Provider answers for the parsing tests.

- `ol-*.json`: recorded from openlibrary.org on 2026-11-11 (search for "the
  left hand of darkness", trimmed to 3 results; `/isbn/9780441478125.json`;
  `/works/OL59858W/editions.json`, trimmed to 5 entries).
- `gb-quota.json`, `gb-badkey.json`: recorded from the Google Books API on
  2026-11-11 (a used-up anonymous quota, and a made-up key).
- `gb-volumes.json`: hand-made in Google's documented volume format, since
  the anonymous quota was used up when recording. Replace it with a recorded
  answer when a key is at hand.

Hardcover and ISBNdb need keys; their parsing has no recorded answers yet.
