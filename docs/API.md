# mau's API

Programs can read and change a reader's library through `/api/v1` on any mau
instance (for example `https://maubooks.org/api/v1`). Every request carries a
bearer token:

    Authorization: Bearer mau_pat_…

There are two ways to get one:

- **A personal access token**, for your own scripts: Settings › Access
  Tokens › Generate a token. Choose what it may do and when it expires. It's
  shown once.
- **"Sign in with mau"** (OAuth 2.1 with PKCE), for an app other people use,
  or one that shouldn't ask anyone to paste a secret. See below.

The API answers JSON. Errors look like
`{"error": "insufficient_scope", "message": "…"}`; `message` can be shown to
people as is. Any website may call the API from the browser: CORS allows every
origin, and no cookies are involved.

[Common tasks](#common-tasks) shows how to add books (with editions, copies
and covers), track reading and page through a library.
[Endpoints](#endpoints) is the full reference.

## Common tasks

The examples use curl with two variables:

    export MAU=https://maubooks.org
    export TOKEN=mau_pat_…

Answers are trimmed to what matters. Every request that sends JSON needs
`Content-Type: application/json`.

### Check the token

    curl -H "Authorization: Bearer $TOKEN" $MAU/api/v1/profile

    {"id": "…", "username": "reader", "display_name": "A Reader", "time_zone": "America/New_York", …}

A 401 means the token is wrong, expired or revoked. A 403 means it lacks a
scope; the answer names the scope it needs.

### Add a book by ISBN (the simplest way)

Send the ISBN. mau finds the work, its editions and its cover:

    curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/books -d '{"isbn": "9780141439518"}'

    201 Created
    {
      "id": "01a21f23-8d65-…", "work_id": "01a21f23-8d60-…",
      "title": "Pride and Prejudice", "authors": ["Jane Austen"], "first_year": 1813,
      "cover": {"small": "https://maubooks.org/covers/d470…-s.jpg", "medium": "…-m.jpg", "large": "…-l.jpg"},
      "editions": [
        {"id": "01a21f23-8d61-…", "isbn13": "9780141439518", "publisher": "Penguin Books",
         "binding": "Paperback", "year": 2003, "pages": 435},
        …
      ],
      "state": "inactive", "tags": [], "copies": [], "reading": null, …
    }

Hyphens and ISBN-10s are fine (`"0-14-143951-3"`).

- **An ISBN mau doesn't know:** 404. Add the book by hand instead (see below).
- **A mistyped ISBN:** 400 with `"error": "isbn"` (the check digit is wrong).
- **A book already on the list:** 409 with `"error": "exists"`; the message
  includes the book's ID.

**Record the copy you own at the same time.** It needs `copies:write`; tags
need `tags:write`:

    curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/books -d '{
        "isbn": "9780141439518",
        "tags": ["classics"],
        "copy": {"format": "Paperback", "location": "Living room"}
      }'

    "copies": [{"id": "…", "format": "Paperback", "location": "Living room",
                "edition_id": "01a21f23-8d61-…"}]

The copy's `edition_id` is the edition with the ISBN you sent: here the 2003
Penguin paperback. A barcode scanner app needs only this one request per book.
If any part is refused (a scope, a tag, the copy), nothing is added.

### Add a book by title or author

Search first (`catalog:search`), then add the result you want by its
`provider` and `source_id`:

    curl -H "Authorization: Bearer $TOKEN" "$MAU/api/v1/catalog/search?q=middlemarch+eliot"

    {"isbn": null, "sources": ["Open Library"], "errors": [],
     "results": [
       {"provider": "openlibrary", "source_id": "OL20867W", "title": "Middlemarch",
        "authors": ["George Eliot"], "cover_url": "https://covers.openlibrary.org/…",
        "edition_count": 332, "book_id": null},
       …]}

    curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/books -d '{"catalog": {"provider": "openlibrary", "source_id": "OL20867W"}}'

- **`book_id`** is set when the reader already has that book: don't add it again.
- **Search results stay usable for 30 days.** Adding one you never searched
  for, or one that has expired, answers 404 "Search again".
- **To pick the edition**, add `"isbn"` to `catalog` (for example
  `{"provider": "openlibrary", "source_id": "OL20867W", "isbn": "9780393877199"}`).
  A `copy` sent with it gets that edition.
- **Searching by ISBN works too** (`q=978-0-14-143951-8`): the answer's
  `isbn` is the normalized ISBN-13.

### Choose an edition for a copy

Every book lists its `editions`. Put one's `id` on a copy, when adding the
book or later:

    curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/books/$BOOK/copies -d '{"format": "Ebook", "edition_id": "01a21f23-ffa7-…"}'

    201 Created
    {"id": "$COPY", "format": "Ebook", "location": "", "edition_id": "01a21f23-ffa7-…"}

    curl -X PATCH -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/copies/$COPY -d '{"location": "Kobo"}'

- **`format`** is one of `Hardcover`, `Paperback`, `Ebook` or `Audiobook`.
- **An edition from another book** answers 400 with `"error": "edition_id"`.
- **`"edition_id": null`** (or `""`) means the edition isn't known.
- **To match a scanned ISBN**, compare it with the editions' `isbn13`.
  Several editions may lack an ISBN or page count; providers don't always
  know them.

### Add a book no source knows

    curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
      $MAU/api/v1/books -d '{
        "manual": {"title": "Family Recipes", "authors": ["Grandma"], "first_year": 1962},
        "copy": {"format": "Hardcover", "location": "Kitchen"}
      }'

A manual book is private to the reader. It has no cover and no editions, so
its copy's `edition_id` is null.

### Show covers

A book's `cover` has three sizes (`small`, `medium` and `large`), or is
`null` when no source had one. They're ordinary image URLs on the mau
instance:

- **No token is needed** to load them: use them directly in `<img src>`.
- **A cover URL never changes** (it's served `immutable`), so it can be
  cached forever.
- **The cover comes from the catalog,** downloaded when the book is first
  added: the work's cover, or else one of its editions'. It doesn't follow a
  copy's edition.
- **A reader's own cover comes first:** one they uploaded on the book's page
  in mau. The API can't upload or change covers.

### Track reading

    # Start (or resume, or re-read); a new reading can give the page count.
    curl … -X POST $MAU/api/v1/books/$BOOK/start -d '{"total": 880}'
    # Log pages: read 40 more today, or reached page 120 yesterday.
    curl … -X POST $MAU/api/v1/books/$BOOK/progress -d '{"add": 40}'
    curl … -X POST $MAU/api/v1/books/$BOOK/progress -d '{"position": 120, "date": "2026-11-26"}'
    # Finish (also happens on reaching the last page), or pause.
    curl … -X POST $MAU/api/v1/books/$BOOK/finish -d '{}'
    curl … -X POST $MAU/api/v1/books/$BOOK/pause -d '{}'

Here `curl …` means curl with both headers. Each answers the Book, with
`"reading": {"started_on": "2026-11-27", "finished": false, "unit": "pages",
"position": 120, "total": 880, …}`. Dates are `YYYY-MM-DD`, by default today
in the reader's time zone. For an ebook read by percent, start with
`{"unit": "percent"}` and log a `position` from 0 to 100.

### Rate, note and tag

    curl … -X PATCH $MAU/api/v1/books/$BOOK -d '{"rating": 5, "notes": "Dorothea!"}'
    curl … -X PUT $MAU/api/v1/books/$BOOK/tags/victorian
    curl … -X DELETE $MAU/api/v1/books/$BOOK/tags/victorian

A `rating` is 1 to 5 stars, or 0 to clear it. Tags are added and removed one
at a time, by name in the path.

### Read the whole library

Books come in pages: pass each answer's `next` as `after` until it's `null`.

```js
const headers = { Authorization: `Bearer ${token}` };
let books = [], after = '';
do {
  const page = await (await fetch(
    `${mau}/api/v1/books?limit=200${after && `&after=${after}`}`, { headers })).json();
  books.push(...page.books);
  after = page.next;
} while (after);
```

Add `state=active` (being read), `inactive` or `finished` to list only those.
The fields a book includes depend on the token's scopes: no `notes` without
`notes:read`, no `copies` without `copies:read`, and so on.

### Errors and limits

| Status | `error` | What to do |
|---|---|---|
| 400 | the field at fault (`isbn`, `edition_id`, `book`, …) | Fix the request; `message` says how |
| 401 | `unauthorized`, `invalid_token` | Get a new token |
| 403 | `insufficient_scope` | Ask for the scope in `WWW-Authenticate` |
| 404 | `not_found` | The book, copy or search result isn't there |
| 409 | `exists`, `not_reading` | Already on the list; not being read |
| 429 | `rate_limited` | Wait a minute |

Each token may make 120 requests a minute. Searches and ISBN lookups at the
sources share the reader's 30 a minute; ISBNs already in mau's catalog don't
count.

## Scopes

| Scope | Allows |
|---|---|
| `profile:read` | Display name, username, bio and time zone |
| `books:read` | Books, with their tags and when they were added |
| `books:write` | Adding books; changing the title and authors the reader sees |
| `books:delete` | Removing books (with their readings, notes and copies) |
| `tags:write` | Adding and removing tags |
| `reading:read` | Readings and the pages read each day |
| `reading:write` | Starting, pausing and finishing books; logging progress |
| `ratings:read`, `ratings:write` | Ratings |
| `notes:read`, `notes:write` | Private notes |
| `copies:read`, `copies:write` | Copies owned and where they're kept |
| `catalog:search` | Searching the catalog (shares the reader's 30 a minute) |
| `shares:read`, `shares:write` | What the reader publishes |

A `:write` scope includes its `:read`. Nothing else is reachable with a
token: not the account (password, two-factor, sessions, recovery codes, tokens,
connected apps), imports, invites, or admin. Without a scope, an endpoint
answers 403 with `WWW-Authenticate: Bearer error="insufficient_scope",
scope="…"`.

Each token may make 120 requests a minute.

## Endpoints

### Profile

`GET /api/v1/profile` (profile:read)

    {"id": "…", "username": "rri", "display_name": "R. Reader", "bio": "…", "time_zone": "Europe/Berlin"}

### Books

`GET /api/v1/books?state=active|inactive|finished&limit=50&after=<next>`
(books:read). Pages of up to 200 books, by ID; pass the answer's `next` as
`after` for the next page (`null` at the end).

    {"books": [Book, …], "next": "…"}

A **Book**:

    {
      "id": "…", "work_id": "…",
      "title": "Middlemarch", "authors": ["George Eliot"],
      "people": [{"name": "George Eliot", "role": "author"}],
      "first_year": 1871,
      "cover": {"small": "https://…/covers/…-s.jpg", "medium": "…", "large": "…"} | null,
      "editions": [Edition, …],      // the work's editions, for a copy's edition_id
      "state": "active" | "inactive" | "finished",
      "added_at": 1763500000000,
      "tags": ["classics"],
      "rating": 4 | null,            // with ratings:read
      "notes": "…",                  // with notes:read
      "copies": [Copy, …],           // with copies:read
      "reading": Reading | null      // with reading:read: the current (latest) reading
    }

An **Edition**: `{"id", "isbn13", "publisher", "binding", "year", "pages"}`
(any but `id` may be null). A manual entry has none.

A **Reading**: `{"id", "started_on", "finished", "finished_on", "unit":
"pages" | "percent", "position", "total", "pages"}`. Dates are `YYYY-MM-DD`;
`finished_on` is null while reading or when the date isn't known. A percent
reading's `total` is 100 and `pages` is the copy's page count, if known.

`GET /api/v1/books/{id}` (books:read): one Book.

`POST /api/v1/books` (books:write; `tags` needs tags:write, `copy` needs
copies:write). One of:

    {"isbn": "9780441013593"}                                // ISBN-10 or -13
    {"catalog": {"provider": "openlibrary", "source_id": "OL…W", "isbn": null}}  // a search result
    {"work_id": "…"}                                         // a work in the catalog
    {"manual": {"title": "…", "authors": ["…"], "first_year": 1871}}             // private to the reader

plus optionally `"tags": ["…"]` and a copy you own:
`"copy": {"format": "Paperback", "location": "Hall", "edition_id": null}`.

- **By ISBN** the work comes from the shared catalog when an edition there
  has that ISBN; otherwise it's looked up the way a catalog search would
  (only Google Books with your own key, if you've added one; otherwise the
  instance's providers in order), counts as one search, and joins the
  shared catalog. 404 if no source knows the ISBN; 400 for a mistyped one.
- **From a search result or by ISBN**, the work is built from the
  provider's own record: title, authors, first year, up to 20 editions and
  the cover, which mau downloads and serves itself. Nothing an app sends
  reaches the shared catalog.
- **A copy without an `edition_id`** gets the edition with the ISBN you
  added by (`isbn`, or the search result's `isbn`), if there is one.

The book goes to the top of the inactive books. Answers 201 with the Book
(or `{"ok": true}` without books:read); 409 if it's already on the list. If
any part is refused (a tag, the copy), nothing is added.

`PATCH /api/v1/books/{id}`: `title` and `authors` (books:write; `""` and `[]`
go back to the catalog's), `rating` 0–5 (ratings:write; 0 clears), `notes`
(notes:write).

`DELETE /api/v1/books/{id}` (books:delete): removes the book with all its
history.

`PUT` / `DELETE /api/v1/books/{id}/tags/{tag}` (tags:write).

### Copies

`POST /api/v1/books/{id}/copies` (copies:write):
`{"format": "Hardcover" | "Paperback" | "Ebook" | "Audiobook", "location": "…", "edition_id": null}`.
`PATCH` and `DELETE /api/v1/copies/{id}`. An `edition_id` must be one of the
book's `editions` (400 otherwise); `""` or null is an unknown edition.

### Reading

`GET /api/v1/books/{id}/readings` (reading:read): every reading, oldest
first, each with its `events`: `{"date", "from", "to", "unit"}`.

These follow the app's own rules (reading:write). Each takes an optional
`"date": "YYYY-MM-DD"`, by default today in the reader's time zone.

- `POST /api/v1/books/{id}/start`: resumes a started book, starts a
  never-started one, or re-reads a finished one; it becomes active, after
  the other active books. A new reading takes `{"total": 304}` (pages) or
  `{"unit": "percent", "pages": 304}`; by default the last reading's way, or
  the catalog's page count, or percent.
- `POST /api/v1/books/{id}/pause`: inactive, at the top of the inactive books.
- `POST /api/v1/books/{id}/finish`: finished that day. The position stays,
  so nothing counts as read.
- `POST /api/v1/books/{id}/progress`: `{"position": 140}` or `{"add": 10}`
  (negative moves back), and optionally `"total"` (a new page count). The
  pages count as read that day; reaching the total finishes the book. 409 if
  the book isn't being read.

### Catalog and shares

`GET /api/v1/catalog/search?q=…` (catalog:search): `{"isbn", "results":
[{"provider", "source_id", "title", "authors", …, "book_id"}], "sources",
"errors"}`. Add a result with `POST /api/v1/books` and its `provider` and
`source_id`.

`GET`, `POST /api/v1/shares`; `PATCH`, `DELETE /api/v1/shares/{id}`
(shares:read, shares:write), as in Settings › Sharing: `{"kind": "finished" |
"active" | "tag" | "catalog", "tag": null, "include_ratings": false}`. Notes
are never published.

## Sign in with mau (OAuth)

Discovery: `GET /.well-known/oauth-authorization-server`.

An app is known by its address, its `client_id`: an `https://` URL on a
domain name (or `http://localhost:<port>/…` while building it). There's
nothing to register. People see the app's host on the consent screen, so the
token always goes where they were told.

An app may send people back only to its own origin (scheme, host and port).
A native app that needs a loopback (`http://127.0.0.1/cb`, any port) or its
own scheme (`com.example.app:/cb`) lists them in a JSON document served at
its `client_id`:

    {"client_id": "https://stats.example.com/", "client_name": "Reading Stats",
     "redirect_uris": ["http://127.0.0.1/callback", "com.example.stats:/callback"]}

mau reads it (https on port 443, public addresses only, 16 KB at most) and
caches it for a day. `client_name` is shown beside the host, never in its
place.

### From a static website

```js
const mau = 'https://maubooks.org';
const b64 = (bytes) => btoa(String.fromCharCode(...bytes))
  .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

// 1. Sign in: send the browser to mau.
const verifier = b64(crypto.getRandomValues(new Uint8Array(32)));
const challenge = b64(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))));
const state = b64(crypto.getRandomValues(new Uint8Array(16)));
sessionStorage.setItem('mau', JSON.stringify({ verifier, state }));
location.assign(`${mau}/oauth/authorize?` + new URLSearchParams({
  response_type: 'code', client_id: 'https://stats.example.com/',
  redirect_uri: 'https://stats.example.com/callback.html',
  scope: 'profile:read books:read reading:read', state,
  code_challenge: challenge, code_challenge_method: 'S256',
}));

// 2. On callback.html: check state and iss, then exchange the code.
const p = new URLSearchParams(location.search);
const saved = JSON.parse(sessionStorage.getItem('mau'));
if (p.get('state') !== saved.state || p.get('iss') !== mau) throw new Error('not from mau');
history.replaceState(null, '', location.pathname);
const tokens = await (await fetch(`${mau}/oauth/token`, {
  method: 'POST',
  body: new URLSearchParams({
    grant_type: 'authorization_code', code: p.get('code'),
    redirect_uri: 'https://stats.example.com/callback.html',
    client_id: 'https://stats.example.com/', code_verifier: saved.verifier,
  }),
})).json();
// { access_token, token_type: "Bearer", expires_in: 3600, refresh_token, scope }
```

`response_mode=fragment` puts the answer after `#` instead of `?`, so it never
reaches the site's server logs.

### The rules

- `/oauth/authorize` needs `response_type=code`, `client_id`, `redirect_uri`
  (exactly as it will be sent to `/oauth/token`), `scope`, `state` (up to 500
  characters) and PKCE with `code_challenge_method=S256`. A bad `client_id`
  or `redirect_uri` gets an error page at mau; other problems come back to
  the app as `error` (`invalid_scope`, `invalid_request`,
  `unsupported_response_type`).
- The reader allows all the scopes asked for, or none (`error=access_denied`).
  Every answer carries `iss`, the instance's address: check it.
- Codes work once, for 60 seconds. Using one twice ends the access it gave.
- Access tokens (`mau_at_…`) last an hour. Refresh with
  `grant_type=refresh_token`, `refresh_token`, `client_id` (and optionally a
  narrower `scope`). Each refresh token works once: using an old one ends the
  app's access for that person, so keep only the newest.
- There are no client secrets; `/oauth/token` refuses one.
- `POST /oauth/revoke` with `token` and `client_id`: a refresh token ends the
  app's access for that person; an access token ends only itself. A
  personal token sent here is revoked too, so whoever finds a leaked one can
  end it.
- The token endpoint and authorize allow 60 requests a minute per address.

People see connected apps in Settings › Access Tokens and can disconnect
them. Setting a password through account recovery ends every token and
connection; a suspended account's tokens stop working; an admin can block an
app's address.
