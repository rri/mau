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
- Passkeys only (WebAuthn, discoverable credentials). No passwords, no OAuth.
- Sign-in screen: a single **Sign in with a passkey** button, a **Use a
  recovery code** link, and **Create an account** only when signup is open.
  Otherwise it says "New here? Ask for an invite link."
- Sessions: server-side, 30 days rolling, viewable and revocable in Settings.

### Signup
- Signup mode is set by an admin: open, invite-only (default), or closed.
- Invite links: single-use, expire after 7 days, revocable. Who can invite is
  set by an admin (default: admins only; optionally every user, with a
  per-user limit on pending invites).
- Flow: choose username (availability checked live) and display name → create
  a passkey (name suggested from browser and OS, editable) → recovery codes
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
- 10 single-use recovery codes. Regenerating (needs a passkey) voids the old set
  and shows the new set once. Settings shows how many are left.
- Recovery-code sign-in asks for username and code. Afterwards the user must
  add a new passkey before doing anything else.
- Last resort: an admin can issue a one-time enrollment link (works once, 24 h).
  It's recorded in the audit log and shown to the user under Recovery activity.

### Passkey confirmation required for
Adding or removing a passkey, regenerating recovery codes, deleting the
account, and changing the username.

### Passkeys
- Several named passkeys per account; rename and remove in Settings. The last
  one can't be removed.

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

## 8. Settings

A list of sections, each its own page, then **Sign out**.

- **Account:** display name, short bio (for the public profile), username,
  time zone, appearance (System / Light / Dark, remembered per device), and
  **Delete account** (offers export first, type the username, confirm with a
  passkey; immediate).
- **Passkeys**, **Sessions** (device and browser, last active; no IP or
  location; sign out one or all others), **Recovery codes** (count, meter,
  regenerate, recovery activity).
- **Sharing:** finished books, currently active, a tag, or the whole catalog.
  Each is Private (default) or Public. Ratings optional per item; notes never.
  "Make a new link" breaks the old one. Sharing with specific people is postponed.
- **Import & export:** import from Goodreads (CSV), StoryGraph (CSV),
  LibraryThing (TSV/JSON), or any CSV with column mapping (needs Title +
  Authors, or ISBN). Review before commit: matched / check match / not found
  (add as manual entry or skip) / already on list (skipped, nothing merged).
  Unfinished books come in inactive; read books without a date are finished
  with an unknown date; shelves become tags verbatim. Export: full JSON
  (re-importable) or simple CSV.
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
- Nothing published: the profile shows "Nothing shared yet".
- Search engines may index profiles and shared pages.

## 10. Visual design

- "Paper and print": warm paper background, Newsreader typeface throughout
  (self-hosted), deep teal accent (#1f5e5b light, #7fc4bf dark).
- Follows the system light or dark mode, with an override in Settings.
- Compact density: 60px rows, 44px round **+**.
- Bottom tab bar: Reading · Catalog · Settings. On wide screens it becomes a
  left sidebar.
- Covers: real covers everywhere. Without one, a generated image that looks
  like a book cover with the title and author.
- Reference: the styled progress view mock.

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
  queries, built-in migrations). **WebAuthn:** webauthn-rs.
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
  end-to-end tests (Reading view, signup with a virtual authenticator,
  offline sync).
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
5. **Rate limits** on sign-in, recovery-code attempts, invite redemption, and
   catalog search, to slow down guessing and protect provider quotas.
6. **Provider responses** are cached on the server for 30 days.
7. **Placement:** time zone and appearance settings go in Account (neither
   was in the Settings mock).
8. **Postponed:** sharing with specific people, lending, and a hosted
   repository with CI.
