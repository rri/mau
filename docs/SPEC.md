# mau: product and technical specification

mau is a self-hosted personal library and reading tracker. This document
collects every decision made during the design phase (2026-11-01). The
interactive mocks in `mocks/` show the approved behavior; where a mock and this
document disagree, this document wins.

## 1. Product basics

- Responsive web app, installable (PWA), works fully offline.
- Self-hosted. Multi-user: one instance can serve many people.
- Private by default. Each user can publish parts of their library publicly.
- No email anywhere: no addresses collected, no mail sent.

## 2. Accounts and authentication

### Sign-in
- Username and password, with optional two-factor codes from an authenticator
  app (TOTP). No passkeys, no OAuth. (Passkeys were the first design; see
  docs/DECISIONS.md, "After launch".)
- Sign-in screen: **Username**, **Password**, **Sign in**, a **Forgot your
  password?** link, and **Create an account** only when signup is open.
  Otherwise it says "New here? Ask for an invite link." With two-factor on, a
  second step asks for the app's 6-digit code; a recovery code works instead.
- Sessions: server-side, 30 days rolling, viewable and revocable in Settings.

### Passwords
- At least 10 characters, at most 256. No rules about character types. Refused:
  the most common passwords (a list built into the server) and the username.
  Stored as Argon2id hashes.
- Changing it (Settings → Password) keeps other devices signed in. Setting a
  new one through recovery signs out every device.

### Two-factor
- Optional, per account (Settings → Two-factor). Setup shows a QR code and the
  key as text; it's on once the app's first code is entered.
- Codes: 6 digits, 30 seconds, SHA-1 (what every common app supports); the
  codes just before and after the current one are accepted too; each code
  works once.

### Signup
- Signup mode is set by an admin: open, invite-only (default), or closed.
- Invite links: single-use, expire after 7 days, revocable. Who can invite is
  set by an admin (default: admins only; optionally every user, with a
  per-user limit on pending invites).
- Flow: choose username (availability checked live) and display name → create
  a password (typed twice) → recovery codes
  shown once; "Start using mau" stays disabled until "I've saved these codes"
  is ticked → empty Reading view with **Add your first book** and **Import from
  Goodreads, StoryGraph…**. No tour.
- Expired or used invite: explains why and says to ask the inviter for a new link.
- First admin: on first start the server prints a one-time setup link to its
  log. The account created through it becomes admin. The link expires on first
  use; a new one is printed on each restart until an admin exists.

### Usernames
- 3–30 characters: a–z, 0–9, `_`, `-`. Case-insensitive. Reserved words blocked.
- Changeable. The old name stays reserved for that user for 30 days. The old
  profile address stops working immediately; per-item share links keep working.

### Recovery
- 10 single-use recovery codes, shown once at signup. Regenerating voids the
  old set and shows the new set once. Settings shows how many are left.
- **Forgot your password?** asks for username, a recovery code, and a new
  password, then signs in. Two-factor stays as it was.
- A recovery code also works in place of an authenticator code.
- Last resort: a one-time reset link (works once, 24 h) from an admin
  (Settings → Admin → Users) or the server command `mau reset-password
  <username>`. It sets a new password and turns two-factor off. It's recorded
  in the audit log and shown to the user under Recovery activity.

### Confirmation required for
Changing the password or username, turning two-factor on or off, regenerating
recovery codes, and deleting the account: the password, plus an authenticator
(or recovery) code when two-factor is on. Valid for 10 minutes.

## 3. Data model

- **Work → Edition → Copy.** A shared catalog of works and editions, with
  per-user overrides (title, authors, cover) that only that user sees.
- **People on a work:** all authors are equals (no co-author). Translator,
  editor, and illustrator are separate non-author roles. Lists and covers show
  authors only; the detail page shows the others. Author sort uses the
  first-listed author's surname.
- **Library entry** (a user's book): manual order position, active flag, tags,
  rating (1–5 whole stars, per book), notes (per book), copies, readings.
- **Copy:** format (hardcover, paperback, ebook, audiobook), location (free
  text), optional edition. Format and edition are chosen separately. A book is
  "owned" when it has at least one copy.
- **Reading:** start date, finish date, total pages, current page, and a log of
  progress events stored with the user's local date. Re-reading creates a new
  reading.
- **Tags:** free-form, case-sensitive, matched verbatim. Autocomplete
  (case-insensitive substring, starts-with first) is the only guard against
  near-duplicates.
- **ISBNs:** ISBN-10 and ISBN-13 accepted, check digit validated, stored as
  ISBN-13.
- **Time zone:** per user, detected from the browser at signup, editable in
  Account. "Today" and calendar days use it.
- **States:** a book is *active*, *inactive* (includes never started and
  paused), or *finished*. There's no "did not finish"; abandoned books just stay
  inactive.

## 4. Reading view (progress view)

- One continuous list of all unfinished books: active books first, then
  inactive. No section headers. Finished books are hidden.
- **Ordering:** the whole list is one manual order (a drag handle on every
  row). Nothing reorders on its own: **+**, page edits, and edits on inactive
  books never move a row.
  - Play moves a book to the end of the active books. Pause moves it to the top
    of the inactive books. A newly added book goes to the top of the inactive
    books.
  - Dropping an inactive book between active books activates it there.
    Dragging an active book down keeps it active and moves it to the end of the
    active books. Dropping at the edge between the groups changes no one's state.
- **Progress is always in pages,** with a total entered by the user. The
  wording is "page 140 of 304".
- **+** appears only on active rows and adds 1 page. Each change shows an undo
  toast for 10 seconds.
- Tapping the progress opens two inline fields, current page and total pages.
  Moving back is a rewind within the same reading.
- First activation of an unstarted book (via play, or dragging it between
  active books) opens inline setup asking for total pages, with a tappable
  "Catalog lists N pages" hint and the field starting empty. It starts at
  page 0 and logs nothing.
- Lowering the total to or below the current page caps the page and finishes
  the book (with undo).
- Reaching the total finishes the book automatically (undo toast) and the row
  leaves the list.
- Rows show cover thumbnail, title, author, thin progress bar, "page X of Y",
  and "+N pages today" or "last read …". Active rows have a teal edge, a pause
  button, and a solid round **+**. Inactive rows are dimmed and have a play button.
- **Search** (always visible under the header): title or author, among the
  unfinished books only, with "N of M books" and **Clear search** while typed.
  It starts empty on each visit. Drag handles hide while searching; +, pause,
  play and page edits still work on the results.
- **List or grid**, switchable in the header and remembered on this device
  separately from the Catalog's. Grid tiles keep the same order and show the
  cover, a progress bar and "p. X of Y" if started, and the title (two lines
  at most). Active tiles have pause and a round **+** on the cover; inactive
  tiles are dimmed and have play. Tapping the cover or title opens the book
  page. Page edits and reordering are list only. Play on a never-started tile
  asks for the total pages in a small dialog (with the catalog hint).

## 5. Catalog view

- Every book on the user's list, including finished ones.
- Layout: list or grid (cover artwork with title below), switchable.
- Filters: state chips **Active / Inactive / Finished** (several can be
  selected, all on by default), an **Owned?** checkbox on the same row
  (unticked shows everything), text search on title and author, and tag chips
  (several tags must all match). Turning off every state chip is allowed and
  shows an empty list with a hint. "Clear filters" resets.
- Sort: recently added (default), title (ignores leading The/A/An), author
  (first author's surname), recently finished (unfinished books after, newest
  added first).
- Layout and sort are remembered; filters reset on each visit. A count line
  ("16 books" / "3 of 16 books").
- List rows: cover, title, author, state ("Inactive" with pages if any
  progress; "Finished · date" with only the latest date), up to two tags, and a
  shelf icon if owned.
- Grid tiles: cover with badges (active, finished, owned), progress bar if
  started and unfinished, title below.
- **+** in the header opens Add a book.

## 6. Book detail page

- Header: cover, title, author, first publication year. A ⋯ menu with **Edit
  details** (title, author, cover upload; a personal override with "Reset to
  catalog") and **Remove from list** (confirmation lists what will be
  deleted; permanent).
- Sections in order: status, rating and notes, tags, copies, reading activity.
- Status: the same activate, pause, **+**, and page edit as the Reading view.
  Activating a finished book starts a re-read at page 0 with the same total
  and returns it to the Reading view.
- Notes save automatically. No catalog description is shown.
- Copies: format, location, and edition line (publisher, binding, year).
  "Add copy" lists editions from the catalog plus any added by ISBN or by hand.
- Manual entries show **Suggest for shared catalog**.
- Reading activity: one calendar for the whole book across all readings, one
  row per year with reading (empty years skipped), shaded 1–9 / 10–24 / 25–49 /
  50+ pages a day, legend "Less … More", tooltips per day. Below it, daily
  summaries grouped per reading, latest expanded, with rewinds as their own lines.

## 7. Adding books

- One search box for title, author, or ISBN (detected automatically). Search
  runs when you press Enter. A **Scan** button opens a live camera scanner
  (zxing-wasm, loaded on demand); typing the ISBN is the fallback.
- Results are works with year and edition count. Books already on your list
  are marked **On your list**; picking one offers "add another copy".
- Picking a result opens a confirm sheet: edition (defaults to the scanned or
  typed ISBN's edition, otherwise "Not sure / choose later"), **I own a copy**
  (off by default; reveals format and location), and tags. Add puts the book at
  the top of the inactive books. You stay on the add screen with a toast and a
  "View" link. The scanner doesn't reopen automatically. No "Start reading now".
- Manual entry: title and author required; more people with roles, first
  published year, cover upload, and edition details optional. Before saving,
  similar catalog titles are offered ("Is it one of these?"). Manual entries
  are private to their creator unless suggested for the shared catalog.
- Metadata providers are configured by an admin: Open Library built in;
  Google Books, Hardcover, and ISBNdb with the admin's own keys. They're tried
  in the admin's order, moving to the next only when results are missing or thin.
- A user with their own Google Books key (Settings → Google Books) searches
  only Google Books, by title or ISBN.
- Covers come with the chosen result. For a Google Books result, Open
  Library's large cover for one of its ISBNs is tried first, since Google's
  are about 128 px wide.

## 8. Settings

A list of sections, each its own page, then **Sign out**.

- **Account:** display name, short bio (for the public profile), username,
  time zone, appearance (System / Light / Dark, remembered per device), and
  **Delete account** (offers export first, type the username, confirm with the
  password; immediate).
- **Password**, **Two-factor**, **Sessions** (device and browser, last active; no IP or
  location; sign out one or all others), **Recovery codes** (count, meter,
  regenerate, recovery activity).
- **Google Books:** the user's own API key. Saving runs one test search and
  refuses a key Google rejects; afterward only "Key ending in …1a2b" shows,
  with Replace and Remove. Never sent back to the browser; not in exports.
- **Sharing:** finished books, currently active, a tag, or the whole catalog.
  Each is Private (default) or Public. Ratings optional per item; notes never.
  "Make a new link" breaks the old one. Sharing with specific people is postponed.
- **Import & export:** import from Goodreads (CSV), StoryGraph (CSV),
  LibraryThing (TSV/JSON), dinky.dev (JSON), or any CSV with column mapping
  (needs Title + Authors, or ISBN). Review before commit: matched / check match
  / not found (add as manual entry or skip) / already on list (skipped, nothing
  merged). Unfinished books come in inactive; read books without a date are
  finished with an unknown date; shelves become tags verbatim. Export: full
  JSON (re-importable) or simple CSV.
- **Import matching:** searches Google Books with the user's own key,
  otherwise Open Library (never the server's Google key). By ISBN when the
  file has one; then title + first author; without a clear match (same title
  ignoring subtitle, notes, a leading article, case and punctuation, and the
  first author's surname among the result's authors), the short title with
  punctuation removed + the surname. If Google refuses a search (quota, bad
  key), Open Library answers for that book and review says how many and why.
  Check-match rows offer what both searches found.
- **Review corrections:** Edit on any row not already on the list: title and
  authors, then Search runs the same search again (not by ISBN). The corrected
  title and authors are used for a manual entry. A corrected row stays in view
  until the tab changes.
- **Saving runs in the background** with "Importing… n of m". Leaving the page
  doesn't stop it; the page picks up the user's latest import (matching,
  review, saving, or the result) when opened again. Done drops it.
- **dinky.dev:** library items ("Title | Author; Author") that aren't deleted.
  Archived items (dinky's done list) are finished with an unknown date, the
  rest inactive in dinky's order (newest first). dinky's created time becomes
  the added date. The first search leaves out trailing (…) notes such as
  "(Dune Series, #2)"; manual entries keep the title as written. No tags.
- **Admin** (admins only): users (make or remove admin, suspend, recovery link,
  delete; the last admin is protected), signup and invites, catalog providers
  (order, keys, on/off), review queue (edit, merge into existing, add to shared
  catalog, decline), shared catalog editor (users' overrides still win for
  them), audit log (kept forever), instance stats.

## 9. Public profile

- `/u/username`: display name, @username, bio, then a section per published
  item. Cover rows (up to 8) with "See all" for finished books and tags;
  progress bars for currently active. Finish month and year; ratings if the
  user chose; pages only on active books. Footer links to the instance.
- Each published item also has its own link.
- Nothing published: the profile shows the same "not found" page as an
  unknown name, so names and bios can't be looked up.
- Search engines may index profiles and shared pages.

## 10. Visual design

- "Clean and crisp": cool neutral greys, white surfaces (near-black in dark
  mode), Geist typeface throughout (self-hosted), vivid emerald accent
  (#047857 light, #34d399 dark). Every text pair is at least 4.5:1.
- Follows the system light or dark mode, with an override in Settings.
- Comfortable density: 64px rows, 42px round **+**, 20px side padding.
- Active books sit on a softly tinted card with an accent bar on the left;
  inactive books have desaturated covers and grey titles. In the grid,
  active covers have an accent outline.
- Bottom tab bar: Reading · Catalog · Settings. On wide screens it becomes a
  sidebar docked to the left of a 720px content panel; the content stays
  centered on screen (from 1120px wide; narrower, the pair is centered).
- Covers: real covers everywhere. Without one, a generated image that looks
  like a book cover with the title and author, lettered in Geist.
- Public pages use the same font and colors.
- Reference: mocks/style2.html (Geist, Emerald, Tinted cards, Docked sidebar).

## 11. Offline and sync

- Every screen works offline. Each device keeps a copy of the user's data.
- Changes are recorded as an operation log (ID, device timestamp with a hybrid
  logical clock) and synced; the server applies them in timestamp order with
  per-action rules.
- Conflicts: the latest change by device time wins, quietly. This includes
  progress: concurrent offline taps on two devices don't add up; the later
  position wins, and the overridden taps don't count in the calendar or
  "today" totals.
- Features that need the server (catalog search, ISBN lookup, import, admin,
  changing shares) show "Needs a connection" offline. Manual entry works
  offline; the similar-title check runs on sync.

## 12. Technical stack

- **Server:** Rust, Axum. **Database:** SQLite via sqlx (compile-time checked
  queries, built-in migrations). **Passwords:** argon2; two-factor codes
  with hmac and sha1.
- **Frontend:** SvelteKit with the static adapter (single-page app), embedded
  in the server binary. No Node at runtime.
- **API:** JSON REST under `/api`, frontend types generated from Rust.
- **Sessions:** random ID in an HttpOnly, Secure, SameSite=Lax cookie; stored
  in SQLite.
- **Covers:** files in the data directory named by content hash, resized to a
  few sizes. Provider covers are downloaded and cached; pages never load
  third-party images.
- **Config:** TOML file (e.g. `/etc/mau/mau.toml`: listen address, public URL,
  data directory, trusted proxies) with `MAU_*` environment overrides. Runtime
  settings live in the database and are changed in the admin UI.
- **HTTPS:** behind the user's reverse proxy. mau listens on plain HTTP and
  trusts forwarded headers only from configured proxy addresses.
- **Deployment:** a single binary (Linux amd64 and arm64). No Docker image.
- **Backups:** daily consistent SQLite snapshot into the data directory,
  keeping the last 7 (configurable), plus a `mau backup <path>` command.
- **Repository:** one repo with a Cargo workspace (`crates/`) and `web/`; one
  build command produces the binary. Local git for now; hosting decided later.
- **Tests:** unit tests (sync and merge rules, ordering, ISBN, import
  parsing), API tests against a temporary SQLite database, and Playwright
  end-to-end tests (Reading view, signup and sign-in, offline sync).
- **License:** AGPL-3.0.

## 13. Assumptions to confirm

These came up while writing this document and haven't been decided explicitly:

1. **Offline copy contents:** each device stores the user's whole library,
   including cover thumbnails (not full-size covers).
2. **Signing out** of a device deletes its offline copy, including unsynced
   changes; mau warns first if any are unsynced.
3. **Revoked sessions:** a device keeps working offline until it next
   connects, then is signed out; its unsynced changes are discarded.
4. **Operations on deleted things** (e.g. + on a book another device removed)
   are dropped silently during sync.
5. **Rate limits** on sign-in (per address and per account), two-factor codes,
   password confirmation, recovery-code attempts, invite redemption, and
   catalog search, to slow down guessing and protect provider quotas.
6. **Provider responses** are cached on the server for 30 days.
7. **Placement:** time zone and appearance settings go in Account (neither
   was in the Settings mock).
8. **Postponed:** sharing with specific people, lending, and a hosted
   repository with CI.
