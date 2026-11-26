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

## Scopes

| Scope | Allows |
|---|---|
| `profile:read` | Display name, username, bio and time zone |
| `books:read` | Books, with their tags and when they were added |
| `books:write` | Adding books; changing the title, authors and cover the reader sees |
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
      "state": "active" | "inactive" | "finished",
      "added_at": 1763500000000,
      "tags": ["classics"],
      "rating": 4 | null,            // with ratings:read
      "notes": "…",                  // with notes:read
      "copies": [Copy, …],           // with copies:read
      "reading": Reading | null      // with reading:read: the current (latest) reading
    }

A **Reading**: `{"id", "started_on", "finished", "finished_on", "unit":
"pages" | "percent", "position", "total", "pages"}`. Dates are `YYYY-MM-DD`;
`finished_on` is null while reading or when the date isn't known. A percent
reading's `total` is 100 and `pages` is the copy's page count, if known.

`GET /api/v1/books/{id}` (books:read): one Book.

`POST /api/v1/books` (books:write; `tags` needs tags:write). One of:

    {"work_id": "…"}                                         // a work in the catalog
    {"catalog": {"provider": "openlibrary", "source_id": "OL…W", "isbn": null}}  // a search result
    {"manual": {"title": "…", "authors": ["…"], "first_year": 1871}}             // private to the reader

plus optionally `"tags": ["…"]`. The book goes to the top of the inactive
books. Answers 201 with the Book (or `{"ok": true}` without books:read); 409
if it's already on the list.

`PATCH /api/v1/books/{id}`: `title` and `authors` (books:write; `""` and `[]`
go back to the catalog's), `rating` 0–5 (ratings:write; 0 clears), `notes`
(notes:write).

`DELETE /api/v1/books/{id}` (books:delete): removes the book with all its
history.

`PUT` / `DELETE /api/v1/books/{id}/tags/{tag}` (tags:write).

### Copies

`POST /api/v1/books/{id}/copies` (copies:write):
`{"format": "Hardcover" | "Paperback" | "Ebook" | "Audiobook", "location": "…", "edition_id": null}`.
`PATCH` and `DELETE /api/v1/copies/{id}`.

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
