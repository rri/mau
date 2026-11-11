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
- Changing it (Settings → Password) signs out every other device; this one
  stays signed in. Setting a new one through recovery or a reset link signs
  out every device.

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
  is ticked → Home ("Welcome, *name*!") with **Add your first book** and
  **Import from Goodreads, StoryGraph…**. No tour.
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
recovery codes, and deleting the account; for admins, also issuing a reset
link, deleting a user, and making someone an admin (each can take over or
remove accounts). The password, plus an authenticator (or recovery) code when
two-factor is on. Valid for 10 minutes. Cancelling changes nothing and says so.

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

## 3a. Landing page

- Visitors who aren't signed in see a landing page at `/`; every other page
  still sends them to sign in. Signed in, `/` is the home page (§3b).
- One centered column in the logo's navy (the same look in light and dark
  mode): the large logo, "mau" in Pinyon Script, the tagline "your personal
  library and reading log" in small caps, a random quote, and **Sign in**
  (plus **Create account** when signup is open). Nothing on the sides.
- The quote is one of 1,028 hand-picked lines of public-domain science
  fiction (authors and translators who died by 1955), a new one on each load,
  shown with author, title (linked to its free Project Gutenberg page) and
  year where known. Source and selection: `crates/mau-server/data/README.md`.
- Footer: a link to github.com/rri/mau · "Source code licensed under
  AGPL-3.0".
- The logo on the sign-in and other public pages links back to it.

## 3b. Home (signed in)

- `/` for signed-in readers; the Reading view moved to `/reading`. Signing in
  or signing up lands here. On wide screens the sidebar logo is the link (it
  gets an accent ring while you're home); phones get a fourth tab, **Home**,
  before Reading.
- Header: today's date, then "Good morning/afternoon/evening, *name*!"
  ("Welcome, *name*!" while the library is empty, with Add and Import), with
  room between the two and below.
- **Pick up where you left off**: up to five active books with progress, the
  round +, and "about N days left at your pace" (from the last 28 days of
  that book; hidden without recent reading). "All N books" opens Reading.
- **Your reading trends** (same heading style), with two subheadings:
  **Last 14 days** (pages per day as bars) and **2026 (so far)** (books
  finished, pages read, days with reading; unknown finish dates don't count).
- No streaks, no "nothing yet today", no per-day average, no monthly chart.
- All figures come from the synced library, so the page works offline.

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
  clean cover, a progress bar, a row with "p. X/Y" (or "Not started") and
  pause and a round **+** (inactive tiles: play, dimmed), then the title (two
  lines at most). Nothing sits on the cover. Tapping the cover or title opens the book
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
- Grid tiles: cover with badges (active or finished top-left, owned
  top-right), progress bar if started and unfinished (its space is always
  kept so titles line up), title below.
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
  item. Cover rows (up to 8, with "See all N" when there are more) for
  finished books, tags, and the whole library;
  progress bars for currently active. Finish month and year; ratings if the
  user chose; pages only on active books. Footer links to the instance.
- Each published item also has its own link (`/s/token`), titled with the
  item's name ("Finished", "Currently reading", "Library", "#tag") and the
  owner, with no second heading above the list. A suspended owner's pages show
  "Nothing here". Shelf titles line up at the top of each row.
- Nothing published: the profile shows the same "not found" page as an
  unknown name, so names and bios can't be looked up.
- Search engines may index profiles and shared pages.

## 10. Visual design

- "Clean and crisp": cool neutral greys, white surfaces (near-black in dark
  mode), Instrument Sans throughout (self-hosted; headings semibold), vivid emerald accent
  (#047857 light, #34d399 dark). Every text pair is at least 4.5:1.
- Follows the system light or dark mode, with an override in Settings.
- Comfortable density: 64px rows, 42px round **+**, 20px side padding.
- Active books sit on a softly tinted card with an accent bar on the left;
  inactive books have desaturated covers and grey titles. In the grid,
  active covers have an accent outline.
- Logo: two Bengal cats reading an open book on a navy badge, no lettering
  (web/static/logo.svg). It's the favicon and every app icon, a badge as wide
  as the tab buttons (176px) at the top of the wide-screen sidebar, and a 112px badge on the sign-in,
  sign-up, invite, reset, and setup pages. Installed-app splash screens are
  navy (#1E242B) to match.
- Bottom tab bar: Home · Reading · Catalog · Settings. On wide screens it becomes a
  sidebar docked to the left of a 720px content panel; the content stays
  centered on screen (from 1120px wide; narrower, the pair is centered).
- Covers: real covers everywhere. Without one, a generated image that looks
  like a book cover with the title and author, lettered in Instrument Sans.
- Public pages use the same font and colors.
- Breadcrumbs above page titles: earlier steps are accent links with a soft
  hover, chevrons between, the current page plain and bold; on phones only
  "‹ parent". A book and Add a book start from the section you came from
  (Home, Reading or Catalog; Catalog when opened directly), and that section's
  tab stays highlighted. Settings pages: Settings › page; admin pages:
  Settings › Admin › page.
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
- **Tests:** see §15.
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

## 14. Behavior by page

Every visible element and what it does, with a behavior ID and the automated
tests that check it. Test codes (details in §15): **C** page or component test,
**U** unit test, **A** API test, **B** Firefox suite, **E** Playwright
end-to-end. Test names start with the behavior ID, so `grep -r HOME-04` finds
them. "—" marks a behavior with no automated test; §15 lists why.

### 14.1 App shell (all signed-in pages)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| SHELL-01 | `/` signed out | Shows the landing page (§14.3), no app chrome. | C |
| SHELL-02 | Any other app page signed out | Replaces the address with `/signin`. | C, B |
| SHELL-03 | Navigation | Wide screens: a sidebar with the 176px logo (link "mau home" to `/`), then Reading, Catalog, Settings, each with an icon. Phones: a bottom tab bar Home · Reading · Catalog · Settings (Home is phones only). The current section has `aria-current="page"`. | C |
| SHELL-04 | Tab on a book or Add a book | The tab of the section the reader came from stays highlighted (Home, Reading or Catalog; Catalog when opened directly). | C |
| SHELL-05 | Settings tab | Highlighted on every `/settings/…` page. On `/` the logo has an accent ring. | C |
| SHELL-06 | Origin | Visiting Home, Reading or Catalog makes it the origin for later books and Add (kept per browser tab, across reloads). | C |
| SHELL-10 | Tab title | Every page sets "*Page* · mau": Home, Reading, Catalog, Add a book, the book's title, each settings page's heading, Sign in, Invitation, New password, Set up mau. The landing page: "mau · your personal library and reading log". | U, C, B |
| SHELL-11 | Unknown address | A card like sign-in: logo, "Page not found", an explanation, and **Go to Home** (signed in) or **Go to the front page**. Tab title "Page not found · mau". | C, B |
| SHELL-12 | Other routing errors | The same card titled "Something went wrong" with the error's message. | C |
| SHELL-20 | Toast | One at a time, bottom of the screen, `role="status"`, 10 seconds with a shrinking timer bar; an optional action (Undo, View) runs once and closes it. A new toast replaces the old one. | C (UI-01), B |
| SHELL-21 | Dialogs | Every dialog and sheet: focus moves in (to its first field or button), Tab and Shift+Tab stay inside, Escape cancels (only the topmost dialog), and focus returns to what opened it. | C (UI-11, BOOK-33, ADD-13, READ-08), B |
| SHELL-22 | Breadcrumbs | Above each page title except the main sections: earlier steps are links (or buttons for in-page steps), chevrons between, the last step plain with `aria-current="page"`. Under 600px only "‹ parent". | C, B |

### 14.2 Home (`/`, signed in)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| HOME-01 | Header | Eyebrow: today's date in the reader's time zone ("Wednesday 11 November"). Heading: "Good night" (before 5), "Good morning" (before 12), "Good afternoon" (before 18), "Good evening", then ", *display name*!", by the device's clock. | C |
| HOME-02 | Loading | "Loading your library…" until the device copy is open. | C |
| HOME-03 | Empty library | Heading "Welcome, *name*!", the text "Your books and reading trends will appear here once you add a book.", **Add your first book** (`/add`) and **Import from Goodreads, StoryGraph…** (`/settings/import`). No sections. | C |
| HOME-04 | Pick up where you left off | Section heading, and "All N books" (N = unfinished books) linking to `/reading`. Each active book in Reading order: cover and title linking to the book, a progress bar, "page X of Y · about N days left at your pace". | C, B |
| HOME-05 | Pace | Days left = pages left ÷ (pages read in this reading in the last 28 days ÷ 28), rounded up, at least 1. Hidden when nothing was read in those 28 days. | U, C |
| HOME-06 | Limit | At most five books. | C |
| HOME-07 | Nothing active | "Nothing in progress right now. Choose a book." (link to Reading). | C |
| HOME-08 | + | Adds one page to that book, with an undo toast "+1 page on *title*". | C |
| HOME-09 | Links | Cover (hidden from screen readers, not a tab stop) and title open the book. | C |
| HOME-10 | Last 14 days | Subheading, 14 bars oldest first; labels every other day plus "today" under the last bar; hover shows "Nov 9: 20 pages" (or "no reading"); a hidden table lists every day for screen readers. | C |
| HOME-11 | *Year* (so far) | Three tiles: books finished this year (unknown dates don't count), pages read this year, days with reading this year. Pages count forward moves only. | U, C |
| HOME-12 | Live | Everything updates as the library changes, online or off. | C |

### 14.3 Landing (`/`, signed out)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| LAND-01 | Column | Logo (220px), "mau" in Pinyon Script, the small-caps tagline, a random quote ("text", author, title linked to its Project Gutenberg page, year), **Sign in**. The quote's space is reserved so the page doesn't jump. Footer: github.com/rri/mau · "Source code licensed under AGPL-3.0". | C, B |
| LAND-02 | Signup open | Adds **Create account**. Without a quote (offline or failed), the rest shows as is. | C |

### 14.4 Signed-out pages

| ID | Element | Behavior | Tests |
|---|---|---|---|
| AUTH-01 | Sign in | Card with the 112px logo (links to `/`), the tagline as heading, **Username**, **Password**, **Sign in** ("Signing in…" while busy). Success goes to Home. | C, B, E |
| AUTH-02 | Wrong password | The server's message in an alert; fields kept. | C, B |
| AUTH-03 | Two-factor step | "Code from your authenticator app" with the recovery-code hint and **Back**. An expired ticket returns to the password step with the message; the password is cleared. | C, B |
| AUTH-04 | Right code | Signs in; each code works once. | C, A, B |
| AUTH-05 | Links | **Forgot your password?** always; **Create an account** only when signup is open, else "New here? Ask for an invite link." | C |
| AUTH-06 | Before setup | "This instance has no admin yet…" notice; no signup link. | C, B |
| AUTH-07 | Signed in already | `/signin` goes to Home. | C |
| AUTH-10 | Forgot password | Username, recovery code, new password twice; success signs in and toasts "New password set. N recovery codes left." (1 code). | C, A, B |
| AUTH-11 | Reset link | "Checking the link…", then "New password for @*name*", a note that other devices sign out and (with two-factor on) that two-factor will be turned off, the new password fields. Success signs in. | C, A, B |
| AUTH-12 | Used or expired reset link | "This link doesn't work" and why. | C, B |
| AUTH-20 | Signup page | Open: the signup flow. Invite-only: "You need an invite link…". Closed: "This instance isn't taking new accounts right now." Both with Back to sign in. | C |
| AUTH-21 | Signup flow | Three dots for the three steps. 1: username (checked 300ms after typing stops; the server's message shows in green or red), display name (defaults to the username); **Continue** only once available. 2: password twice (§AUTH-30), **Create account** ("Creating your account…"); a taken name returns to step 1. 3: ten recovery codes, **Copy all**, "I've saved these codes somewhere safe" enables **Start using mau** (goes Home). | C, B, E |
| AUTH-22 | Invite link | "*Inviter* invited you to mau." then the flow; a used or expired link explains and offers sign-in. | C, B |
| AUTH-23 | Setup link | "Set up this instance", **Create the admin account**, then the flow with the admin notice. A used link explains how to get a new one. | C, B, E |
| AUTH-30 | New password fields | "N more characters needed." under 10; "The two don't match."; **Show password**; the submit button waits for a valid pair. The server also refuses common passwords. | C, A, B |

### 14.5 Reading (`/reading`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| READ-01 | List | Unfinished books: active (tinted, accent edge) by the reader's order, then inactive (dimmed covers, grey titles). | C, U, E |
| READ-02 | Row | Drag handle, cover and title (link), authors, a thin progress bar, then "page X of Y" (a button) with "+N page(s) today", "last read *ago*", or "no progress yet"; never-started: "Not started" and "added *ago*". | C |
| READ-03 | Controls | Active: pause (to the top of the inactive books) and a round + (one page). Inactive: play (resume at the end of the active books). Each shows an undo toast. | C, U, B, E |
| READ-04 | Page editor | The page button opens "page [ ] of [ ]" with Save, focused and selected; Enter or leaving the fields saves, Escape cancels. Saving moves forward or back ("Moved back to page N"), changes the total, and finishes the book at the total. | C, U, B |
| READ-05 | First start | Play on a never-started book opens inline "How many pages does your copy have?" (empty, focused), "Catalog lists N pages" fills it, an invalid number says "Enter the number of pages in your copy.", **Start reading** starts at page 0. | C, B |
| READ-06 | Search | "Search title or author"; while typed, "N of M books" and **Clear search**; drag handles hide (their column stays). Starts empty each visit. | C, B |
| READ-07 | No match | "No books you're reading match "*q*"." | C |
| READ-08 | Grid | Tiles: cover (link), progress bar, "p. X/Y" or "Not started" with pause and + (or play), the title. Play on a never-started tile opens a dialog with the same pages question. | C, B |
| READ-09 | Layout | The List/Grid toggle is remembered on this device, separately from the Catalog's. | C, B |
| READ-10 | Status | Next to the title: "Offline · N unsynced" or "Syncing…". | C |
| READ-11 | Empty | "Welcome, *name*." with Add and Import; no search or layout toggle. | C |
| READ-12 | Loading | "Loading your library…". | C |
| READ-ACT-01…09 | Actions | +, set pages (clamped 0…total), finishing and its undo, pause, resume, start, re-read: the exact changes and toasts. | U |
| READ-DRAG-01…05 | Drag rules | §4 ordering rules for every drop position; a never-started book dropped among active books asks for its pages first. A drop label shows "Move here", "Activate here", "Start here", or "Stays active: goes to end of active". | U (rules); the pointer gesture: — |

### 14.6 Catalog (`/catalog`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| CAT-01 | List | Every book, recently added first: cover, title, authors, state ("Active · page X of Y", "Inactive" with pages if started, "Finished · date"), up to two #tags, a shelf icon "You own a copy". | C, E |
| CAT-02 | Filters | State chips (all on), **Owned?**, tag chips (all chosen tags must match), search; they combine. The count reads "N books" or "n of N books"; **Clear filters** appears while filtering. | C, E |
| CAT-03 | Tag chips | Alphabetical; shown only when there are tags. | C |
| CAT-04 | Search | Title or any author, ignoring case. | C, B |
| CAT-05 | Empty results | No state chosen: "Select Active, Inactive, or Finished to see books."; otherwise "No books match these filters." | C |
| CAT-06 | Sort | Recently added, Title (ignores The/A/An), Author (first author's surname, then title), Recently finished. Remembered on this device. | C, U |
| CAT-07 | Grid | Cover with a finished check or active dot (top left) and an owned badge (top right); a progress strip whose space is always kept; the title. The link's accessible name adds authors, state and ownership. | C |
| CAT-08 | Empty catalog | "Your catalog is empty." with **Add a book**; the header + always opens Add. | C |

### 14.7 Book page (`/catalog/<id>`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| BOOK-01 | Header | Cover, title (h1), authors, "Translated/Edited/Illustrated by *name*", "First published *year*". | C |
| BOOK-02 | Trail | *Origin* › *title*; the origin link goes back in history when the reader came straight from it (keeping its scroll and filters). | C, B |
| BOOK-03 | Not on the list | "This book isn't on your list." with "Back to *origin*". | C |
| BOOK-04 | Private entry | "Only you can see this entry" (or "Suggested for the shared catalog"); ⋯ › **Suggest for shared catalog** toasts that an admin will review it. | C |
| BOOK-05 | ⋯ menu | Edit details, (Suggest), Remove from list; closes on any click outside. | C |
| BOOK-06 | Edit details | Title, "Authors, separated by commas", cover image; saved only for this reader. "You've edited these details. Reset to catalog" undoes all three. | C |
| BOOK-10 | Status | Pill (Active, Inactive, Finished), progress bar, "page X of Y" button, "+N page(s) today"; active: pause and +; otherwise Start, Resume, or Re-read. | C |
| BOOK-12 | Page editor | Current and total page with Save and Cancel. | C |
| BOOK-13 | Start | Asks for the pages like READ-05. | C |
| BOOK-14 | Finished | "Finished *date*"; Re-read starts a new reading at page 0 with the same total. | C, U |
| BOOK-20 | Rating | Five stars (a radio group); choosing the current rating again clears it. | C, B |
| BOOK-21 | Notes | "Private notes about this book"; saved 0.7 seconds after typing stops, then "Saved" by the heading. | C, B |
| BOOK-22 | Tags | Chips with ×; "+ add tag" suggests existing tags (§14.12) or creates one. | C, B |
| BOOK-23 | Copies | Heading "Copies · owned" when there are copies; each: format, location (or "No location"), edition line, ×. Without copies: "You don't own a copy…". **+ Add copy**: format, edition (the work's editions or "Unknown edition", the default), location. Removing toasts "Copy removed". | C, B |
| BOOK-30 | Reading activity | "N day(s) read · N page(s) · N reading(s)", the calendar (one row per year with reading, plus this year while reading; shaded 1–9 / 10–24 / 25–49 / 50+; "Less … More"; hover shows the day), then each reading newest first ("Reading N · finished", dates; the newest open) with its days newest first ("page a → b +n", rewinds as "moved back page a → b"). | C |
| BOOK-31 | No progress | "No progress yet" in a reading; "No reading yet." for a never-started book. | C |
| BOOK-33 | Remove | A dialog "Remove *title*?" listing what's deleted (readings, rating, notes, copies, tags); Remove deletes, toasts "Removed *title* from your list", and returns to the catalog. | C, B |

### 14.8 Add a book (`/add`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| ADD-01 | Start | Trail *origin* › Add a book; search box "Title, author, or ISBN" with **Scan**; help text; "Can't find it? Enter it manually". | C |
| ADD-02 | Results | Status line "[ISBN x ·] N result(s) from *source*, then *source* [· first error]"; each result: cover, title, authors, "*year* · N edition(s)" ("Year unknown"), › or **On your list**. | C, B |
| ADD-03 | Bad ISBN | A mistyped check digit is caught before searching. | C, U |
| ADD-04 | No results, errors | "No books found for "*q*"."; a failed search shows its message. | C |
| ADD-05 | Offline | "Searching the catalog needs a connection. You can still enter a book by hand." | C |
| ADD-10 | Sheet | Cover, title, authors · year; Edition (loading, then the provider's list; "Not sure / choose later" unless an ISBN picked one); **I own a copy** reveals Format (the edition's binding) and Location; Tags. An ISBN with one result opens it straight away. | C |
| ADD-11 | Add | The book goes to the top of the inactive books with its tags and copy; toast "Added *title* to the top of your inactive books[, with a copy]" with View; the sheet closes, the search stays. | C, B (network) |
| ADD-12 | Already on the list | "This book is already on your list. Add another copy, or open its page." and **Add copy**. | C |
| ADD-13 | Closing | Cancel, Escape, or a tap outside. | C |
| ADD-14 | Offline add | "Adding from the catalog needs a connection." | C |
| ADD-20 | Manual entry | Trail *origin* › Add a book › Enter a book (the middle step returns); the search carries over as the title (or the ISBN). | C, B |
| ADD-21 | Checks | "Enter at least a title and an author."; year as a number; a valid ISBN or none. | C |
| ADD-22 | Save | Private work, people with roles (+ Add another person, ×), first year, optional cover and edition details; to the top of the inactive books; toast "Added *title* (private to you)…". | C, B |
| ADD-23 | Similar titles | Once per entry, online: "Is it one of these?" with matching shared works (picking one opens its sheet) and **No, add my book**. | C, B |
| ADD-24 | Cancel | Back to the search. | C |
| ADD-30 | Scanner | Full-screen camera view with a frame; reads EAN-13 ISBNs (978/979) and searches; vibrates on success; ✕, Escape, or "Type the ISBN instead" close it; without a camera or permission it says so. | C (messages and closing); decoding: — |

### 14.9 Settings (`/settings` and personal pages)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| SET-01 | Index | Rows with chevrons: *display name* (@username), Password, Two-factor (On/Off), Sessions, Recovery codes ("N of 10 left"), Sharing ("N shared"), Google Books (Own key/Off), Import & export; admins: an Admin group (anchor `#admin`) with seven pages. | C |
| SET-02 | Variants | Non-admins: no Admin group; **Invites** when they may invite or have pending invites. "None left" for zero codes; "Nothing shared". | C |
| SET-03 | Sign out | With unsynced changes, a dialog explains they'd be lost; otherwise (or "Sign out anyway") the session ends, this device's copy is deleted, and sign-in opens. | C, B |
| SET-10 | Account | Trail Settings › Account; display name, bio (280 characters), username with rules, time zone (with "Use *device zone*" when they differ). | C |
| SET-11 | Saving | Asks for confirmation when needed; a username change toasts that the old one stays reserved for 30 days; otherwise "Saved". | C, A |
| SET-12 | Cancel confirmation | "Cancelled. Nothing was changed." | C |
| SET-13 | Two-factor confirmation | The dialog also asks for an authenticator (or recovery) code. | C, A, B |
| SET-14 | Appearance | Match the system / Light / Dark, on this device only, applied at once. | C |
| SET-15 | Delete account | Offers the export first; a dialog asks to type the username (case and spaces ignored) and confirm; then the account and this device's copy are deleted and sign-in opens. A wrong name deletes nothing. | C, A |
| SET-20 | Password | New password twice; "Password changed. Other devices were signed out."; the fields clear. | C, A, B |
| SET-21 | Refused password | The server's reason. | C, A, B |
| SET-22 | Two-factor on | Confirm, then the QR code and the key in groups of four, then a code; "Two-factor is on." | C, A, B |
| SET-23 | Two-factor off | A dialog, confirmation, "Two-factor is off." | C, A |
| SET-30 | Sessions | Each browser with "This device", "Active now", "Active in the last hour", or "Last active *ago*"; Sign out per other session; Sign out all other sessions. | C, A |
| SET-31 | Failures | Failed actions show their message. | C |
| SET-32 | Recovery codes | "N of 10 codes left" with a meter; "Only N left. Generate a new set soon." at 3 or fewer; "No codes left. Generate a new set now…" at 0; recovery activity with dates or "None." | C, A |
| SET-33 | New codes | Confirm, then the codes once with **Copy all**; **Done** waits for "I've saved them". | C, A |
| SET-40 | Sharing | The intro with the profile address; each share: name, Public, "Includes ratings/No ratings · /s/token", Copy link, Show/Hide ratings, Make a new link, Stop sharing. **Share something**: Finished books, Currently reading, A tag… (starts on the first tag), Whole catalog, Include my ratings. | C, A, B |
| SET-41 | No tags | "A tag… (no tags yet)" can't be chosen; "You're not sharing anything." only after loading. | C |
| SET-42 | Share actions | Each confirms with a toast; failures show their message. | C, A |
| SET-43 | Google Books | Explanation; a key field (Save key tests it first); then "Key ending in …abcd" with Remove and Replace…. | C, A |
| SET-44 | Invites | Each link creates one account within 7 days and is shown once; pending invites with "Expires today/tomorrow/in N days" and Revoke; **Create invite link** when allowed. | C, A, B |
| SET-45 | No permission | "You can't create more invites right now."; no empty message while loading. | C |

### 14.10 Import & export (`/settings/import`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| IMP-01 | Choose | Import from (six sources), File, the note that nothing changes until confirmed, **Read file** (asks for a file if none). | C |
| IMP-02 | Review | "N books in the file", any Google refusal note, tab chips with counts (Matched, Check match, Not found, Already on list); rows with title, authors · summary · #tags; Import checkbox, match choice, manual-or-skip, or "Skipped"; **Import N book(s)** counts what will come in. Saving shows progress, then "Imported N books, skipped M." and Done. | C, A, B (network) |
| IMP-03 | Corrections | Edit → Title and "Authors, separated by ;" → Search; the row stays in view on its tab. An import in progress is picked up when the page opens. | C, A |
| IMP-04 | Other CSV | Columns mapped to fields, guessed from the headers (each field once), with a sample value; Continue starts matching ("Matching books with the catalog… n of N"). | C, A |
| IMP-05 | Offline, export | "Importing needs a connection."; Spreadsheet (CSV) and Full export (JSON) always. | C, A |
| IMP-06 | Failure | "Saving stopped: *reason*" with Done. | C |

### 14.11 Admin (`/settings/admin/…`)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| ADM-01 | Guards | Non-admins: "Only admins can see this." (the API refuses them too); offline: "Admin pages need a connection." | C, A |
| ADM-02 | Trail | Settings › Admin (`/settings#admin`) › page. | C, B |
| ADM-10 | Users | Cards: display name @username, Admin and Suspended pills, "N book(s)"; Make/Remove admin, Suspend/Unsuspend, Reset link, Delete. The last active admin can't be demoted, suspended, or deleted; nobody can suspend or delete themselves here. | C, A |
| ADM-11 | Delete | A dialog (wording for 0, 1, or N books), then password confirmation, then "Deleted @*name*". | C, A |
| ADM-12 | Reset link | A dialog explaining it, confirmation, then the one-time link (24 hours) with Copy and Done. | C, A, B |
| ADM-13 | Admin, suspend | Granting admin needs confirmation; suspending signs the person out everywhere. | C, A |
| ADM-20 | Signup & invites | Who can sign up (Open, Invite only, Closed) and who can invite (Admins only, Every user with a limit), each saved as soon as chosen; then the invite list. | C, A |
| ADM-21 | Providers | Ordered cards with up/down chevrons, a key field (never shown back; "Key saved (type to replace)"), On (only with a key when one is needed), Save. | C, A |
| ADM-30 | Review queue | Each suggestion: title, "from @owner", people · year, editions; "Possibly the same as *title* (*year*). Merge into it"; Edit, then add; Add to shared catalog; Decline. | C, A |
| ADM-31 | Empty queue | "No suggestions waiting." after loading. | C |
| ADM-32 | Shared catalog | Find by title, author, or ISBN; cards with people · year · N edition(s); Edit (title, people, year) saves for everyone and is audited; Replace cover; each edition editable; "No books in the catalog match "*q*"." | C, A |
| ADM-40 | Audit log | Time and "*actor* *action*", newest first, 200 at a time with **Older** (by entry, so nothing is skipped). | C, A |
| ADM-41 | Empty log | "Nothing recorded yet." after loading. | C |
| ADM-42 | Stats | Users, books, shared works, private entries, cover and database sizes. | C |

### 14.12 Shared pieces

| ID | Element | Behavior | Tests |
|---|---|---|---|
| UI-10 | Confirm it's you | Password (and code with two-factor); a wrong one shows the server's message; once confirmed the action runs again; valid 10 minutes. | C, A, B |
| UI-20 | Tag input | Combobox: existing tags containing the text (any case), starts-with first, up to 8, then "Create new tag #x" or "Already tagged #x"; arrows move, Enter picks, Escape clears; a leading # is ignored. | C |
| UI-30 | Covers | The real cover (small, medium, large), lazy-loaded; a generated cover (colors from the title, title and author lettered) when there's none or it fails to load. Hidden from screen readers. | C |
| UI-31 | Bar chart | Bars from a shared baseline scaled to the largest, zero bars flat, the current bar highlighted, hover tips, and a hidden table. | C |
| DATA-01…07 | Book info | State, overrides, ownership, tag order, deleted books, Reading order, pages today, search matching. | U |
| SYNC-01…06 | Device copy | §11: changes show at once and sync shortly after; offline they wait (also across reloads) and go when the connection returns, even if it returns during a failing attempt; rejected changes are dropped with a toast; a revoked session wipes the device and opens sign-in; syncs fetch only what changed. | C, A, E |

### 14.13 Public pages (server-rendered)

| ID | Element | Behavior | Tests |
|---|---|---|---|
| PUB-01 | Profile `/u/name` | Display name, @username, bio, then each published item (§9). Nothing published, unknown, or suspended: "Nothing here" (404). | A, B |
| PUB-02 | Share `/s/token` | §9. Ratings only when chosen; notes never; a new link retires the old one. | A, B |

### 14.14 Server responsibilities with no screen

| ID | Behavior | Tests |
|---|---|---|
| SRV-01 | Every endpoint except sign-in, signup, the setup check, invite and reset lookups, status, health, and quotes answers 401 when signed out; admin endpoints answer 403 to others. | A |
| SRV-02 | State-changing requests from other sites are refused; security headers and a CSP on every response. | A |
| SRV-03 | Rate limits on sign-in, codes, recovery, and confirmation; repeated wrong codes lock the account for a while. | A |
| SRV-04 | Sync: field-level last-writer-wins, progress chains, tombstones, other users' rows untouchable, invalid batches rejected whole. | A, U |
| SRV-05 | Exports contain only the reader's own books, notes included. | A |

## 15. Verification

| Layer | Where | Runs with | Count (2026-11-11) |
|---|---|---|---|
| U: unit | `web/src/lib/*.spec.ts`, `*.svelte.test.ts` for runes modules; Rust `#[test]` | `npx vitest run`, `cargo test` | part of the counts below |
| C: page and component | `web/src/**/*.svelte.test.ts` (jsdom, Testing Library, mock library in `web/src/test/fixtures.ts`, mock API by route) | `npx vitest run --project client` | 206 web tests in all |
| A: API | `crates/mau-server/src/**` tests against a temporary SQLite database (`app_tests.rs`, `access_tests.rs`, and per module) | `cargo test` | 198 Rust tests |
| B: Firefox | `web/browser-tests/` (headless Firefox via puppeteer-core, throwaway seeded servers on free ports; axe-core for WCAG 2.1 AA) | `just browser-test` | 29 steps (+1 with `MAU_BROWSER_NETWORK=1`) |
| E: Playwright | `web/e2e/` (Chromium, for CI) | `just e2e` | 5 |

`just test` runs the unit, page, and API tests with type checks. Every
behavior above has at least one automated test, except:

- **Camera barcode decoding** (ADD-30): needs a camera; the scanner's messages
  and closing are tested, and decoded text goes through the tested ISBN check.
- **The drag gesture** (READ-DRAG): the drop rules are unit-tested; the
  pointer tracking and drop label aren't (jsdom has no layout; Playwright
  could cover it in CI).
- **Real provider calls**: catalog search and import matching call Open
  Library, Google Books, Hardcover, and ISBNdb. Open Library parsing is tested
  against answers recorded on 2026-11-11, and Google Books against its recorded
  refusals (quota, bad key) and a hand-made volume
  (`crates/mau-server/src/providers/testdata/`). Hardcover and ISBNdb need keys
  and have no recorded answers. The order of providers and the fall-back on
  thin results aren't tested. The live Open Library flow runs in the Firefox
  suite only with `MAU_BROWSER_NETWORK=1` (it passed on 2026-11-11).
- **Installing as an app, and the service worker's cached app shell and
  covers**: offline sync is covered by the Playwright offline test, but
  opening the app with no connection at all isn't.
- **Pixel layout**: checked by eye from screenshots; no visual-diff tests.
