//! The public API, `/api/v1`, for bearer tokens only (cookies are never
//! read, so CORS can allow any origin). Writes become the same operations the
//! app syncs, stamped with the server's clock and applied by
//! `library::apply`, so they follow the same validation, ownership and
//! conflict rules, and reach the reader's devices with their next sync.
//! Reading rules follow the app's (web/src/lib/actions.ts).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use super::token::TokenAuth;
use crate::app::AppState;
use crate::auth::session::Authed;
use crate::error::{ApiError, ApiResult};
use crate::importer::UNKNOWN_DATE;
use crate::library::{
    self, Applied, BookFields, ChainCtx, CopyFields, NewWork, Op, OpKind, Person, ReadingFields,
    Unit,
};
use crate::util::{new_id, now_ms};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/profile", get(profile))
        .route("/books", get(list_books).post(add_book))
        .route(
            "/books/{id}",
            get(get_book).patch(edit_book).delete(remove_book),
        )
        .route("/books/{id}/tags/{tag}", put(add_tag).delete(remove_tag))
        .route("/books/{id}/copies", post(add_copy))
        .route("/copies/{id}", patch(edit_copy).delete(remove_copy))
        .route("/books/{id}/readings", get(readings))
        .route("/books/{id}/start", post(start))
        .route("/books/{id}/pause", post(pause))
        .route("/books/{id}/finish", post(finish))
        .route("/books/{id}/progress", post(progress))
        .route("/catalog/search", get(search))
        .route("/shares", get(shares_list).post(shares_create))
        .route("/shares/{id}", patch(shares_update).delete(shares_delete))
        .fallback(not_found)
}

async fn not_found() -> ApiError {
    ApiError::not_found(
        "No such API endpoint. See /.well-known/oauth-authorization-server and docs/API.md.",
    )
}

fn no_book() -> ApiError {
    ApiError::not_found("No such book on your list.")
}

fn bad(code: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError::bad_request(code, msg)
}

/// Today in the reader's time zone (UTC if the zone isn't known here).
fn today(tz: &str) -> String {
    let zone = jiff::tz::TimeZone::get(tz).unwrap_or(jiff::tz::TimeZone::UTC);
    jiff::Timestamp::now().to_zoned(zone).date().to_string()
}

/// A date given by the app, or today. Not after tomorrow (time zones).
fn date_or_today(given: Option<&str>, tz: &str) -> ApiResult<String> {
    let Some(d) = given else { return Ok(today(tz)) };
    let parsed: jiff::civil::Date = d
        .parse()
        .ok()
        .filter(|_| d.len() == 10)
        .ok_or_else(|| bad("date", "Dates are YYYY-MM-DD."))?;
    let latest: jiff::civil::Date = today("UTC").parse().expect("own format");
    if parsed > latest.tomorrow().unwrap_or(latest) {
        return Err(bad("date", "That date is in the future."));
    }
    if parsed.year() < 1900 {
        return Err(bad("date", "That date is too long ago."));
    }
    Ok(d.to_string())
}

// ---------------------------------------------------------------------------
// Writing: operations in one transaction

/// One request's changes: one transaction and change number. State is read
/// inside it (after the write lock is taken), so the operations are built
/// from what's current.
struct Batch {
    tx: Transaction<'static, Sqlite>,
    seq: i64,
}

impl Batch {
    async fn begin(state: &AppState) -> ApiResult<Batch> {
        let mut tx = state.db.begin().await?;
        let seq = library::next_seq(&mut tx).await?;
        Ok(Batch { tx, seq })
    }

    fn conn(&mut self) -> &mut SqliteConnection {
        &mut self.tx
    }

    /// Applies one operation with a fresh server clock. A refused operation
    /// fails the whole request (nothing is saved).
    async fn apply(&mut self, state: &AppState, user_id: &str, kind: OpKind) -> ApiResult<Applied> {
        let hlc = {
            let mut c = state.clock.lock().unwrap();
            c.now(now_ms() as u64).to_string()
        };
        let op = Op {
            id: new_id(),
            hlc,
            op: kind,
        };
        let (infos, mut outcomes) = library::progress_chains(std::slice::from_ref(&op));
        let ctx = infos[0].as_ref().map(|info| ChainCtx {
            info,
            outcomes: &mut outcomes,
        });
        library::apply(&mut self.tx, user_id, self.seq, &op, ctx).await
    }

    async fn commit(self) -> ApiResult<()> {
        self.tx.commit().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// What the API returns

#[derive(Serialize)]
struct Profile {
    id: String,
    username: String,
    display_name: String,
    bio: String,
    time_zone: String,
}

#[derive(Serialize)]
struct Cover {
    small: String,
    medium: String,
    large: String,
}

#[derive(Serialize)]
struct Copy {
    id: String,
    format: String,
    location: String,
    edition_id: Option<String>,
}

#[derive(Serialize, Clone)]
struct Reading {
    id: String,
    started_on: Option<String>,
    finished: bool,
    /// Null while reading, or when the finish date isn't known.
    finished_on: Option<String>,
    /// pages or percent
    unit: &'static str,
    position: i64,
    /// The page count (100 for percent).
    total: i64,
    /// Percent readings: the copy's page count, if known.
    pages: Option<i64>,
}

#[derive(Serialize)]
struct Event {
    date: String,
    from: i64,
    to: i64,
    unit: &'static str,
}

#[derive(Serialize)]
struct ReadingWithEvents {
    #[serde(flatten)]
    reading: Reading,
    events: Vec<Event>,
}

/// An edition of a book's work: what a copy can point at (`edition_id`).
#[derive(Serialize)]
struct Edition {
    id: String,
    isbn13: Option<String>,
    publisher: Option<String>,
    /// Hardcover, Paperback, Ebook, Audiobook, as the source describes it.
    binding: Option<String>,
    year: Option<i64>,
    pages: Option<i64>,
}

/// A book as the API shows it. Fields beyond `books:read` appear only when
/// the token has their scope: `rating` (ratings:read), `notes` (notes:read),
/// `copies` (copies:read) and `reading` (reading:read, the current reading).
#[derive(Serialize)]
struct Book {
    id: String,
    work_id: String,
    title: String,
    authors: Vec<String>,
    /// Everyone named on the work, with their role (author, translator,
    /// editor, illustrator).
    people: Vec<Person>,
    first_year: Option<i64>,
    cover: Option<Cover>,
    /// The work's editions, for choosing a copy's edition.
    editions: Vec<Edition>,
    /// active, inactive, or finished
    state: &'static str,
    added_at: i64,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rating: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    copies: Option<Vec<Copy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reading: Option<Option<Reading>>,
}

type ReadingTuple = (
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
    i64,
    Option<i64>,
    String,
);

fn reading_from(t: &ReadingTuple) -> Reading {
    let (id, _, started_on, finished_on, unit, total, pos, pages, _) = t;
    let unit = Unit::parse(unit);
    Reading {
        id: id.clone(),
        started_on: Some(started_on.clone()).filter(|d| d != UNKNOWN_DATE),
        finished: finished_on.is_some(),
        finished_on: finished_on.clone().filter(|d| d != UNKNOWN_DATE),
        unit: unit.as_str(),
        position: *pos,
        total: *total,
        pages: *pages,
    }
}

/// A user's live readings of these books, oldest first per book (the last is
/// the current one, as in the app).
async fn readings_of(
    conn: &mut SqliteConnection,
    user_id: &str,
    ids_json: &str,
) -> sqlx::Result<HashMap<String, Vec<ReadingTuple>>> {
    let rows: Vec<ReadingTuple> = sqlx::query_as(
        "SELECT id, book_id, started_on, finished_on, unit, total, pos, pages, pos_clock FROM readings
         WHERE user_id = ? AND deleted = 0 AND book_id IN (SELECT value FROM json_each(?)) ORDER BY id",
    )
    .bind(user_id)
    .bind(ids_json)
    .fetch_all(conn)
    .await?;
    let mut by_book: HashMap<String, Vec<ReadingTuple>> = HashMap::new();
    for r in rows {
        by_book.entry(r.1.clone()).or_default().push(r);
    }
    Ok(by_book)
}

/// The books with these IDs (the user's live ones), in that order.
async fn load_books(
    conn: &mut SqliteConnection,
    state: &AppState,
    auth: &TokenAuth,
    ids: &[String],
) -> ApiResult<Vec<Book>> {
    let user_id = &auth.user.id;
    let ids_json = serde_json::to_string(ids).expect("strings serialize");
    type Row = (
        String,
        String,
        bool,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, work_id, active, rating, notes, title_override, authors_override, cover_override, added_at
         FROM books WHERE user_id = ? AND deleted = 0 AND id IN (SELECT value FROM json_each(?))",
    )
    .bind(user_id)
    .bind(&ids_json)
    .fetch_all(&mut *conn)
    .await?;
    let work_ids: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let works: HashMap<String, library::WorkRow> =
        library::load_works(conn, &work_ids, Some(user_id))
            .await?
            .into_iter()
            .map(|w| (w.id.clone(), w))
            .collect();
    let tag_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT book_id, tag FROM book_tags WHERE user_id = ? AND present = 1
         AND book_id IN (SELECT value FROM json_each(?)) ORDER BY tag",
    )
    .bind(user_id)
    .bind(&ids_json)
    .fetch_all(&mut *conn)
    .await?;
    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    for (b, t) in tag_rows {
        tags.entry(b).or_default().push(t);
    }
    let mut copies: HashMap<String, Vec<Copy>> = HashMap::new();
    if auth.allows("copies:read") {
        let rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, book_id, format, location, edition_id FROM copies WHERE user_id = ? AND deleted = 0
             AND book_id IN (SELECT value FROM json_each(?)) ORDER BY id",
        )
        .bind(user_id)
        .bind(&ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (id, book_id, format, location, edition_id) in rows {
            copies.entry(book_id).or_default().push(Copy {
                id,
                format,
                location,
                edition_id,
            });
        }
    }
    let readings = readings_of(conn, user_id, &ids_json).await?;
    let public_url = state.config.public_url.trim_end_matches('/');

    let mut by_id: HashMap<String, Book> = HashMap::new();
    for (id, work_id, active, rating, notes, title_o, authors_o, cover_o, added_at) in rows {
        let Some(work) = works.get(&work_id) else {
            continue;
        };
        let current = readings.get(&id).and_then(|r| r.last());
        let state_name = if current.is_some_and(|r| r.3.is_some()) {
            "finished"
        } else if active {
            "active"
        } else {
            "inactive"
        };
        let cover = cover_o
            .or_else(|| work.cover.clone())
            .or_else(|| work.editions.iter().find_map(|e| e.cover.clone()))
            .map(|h| Cover {
                small: format!("{public_url}/covers/{h}-s.jpg"),
                medium: format!("{public_url}/covers/{h}-m.jpg"),
                large: format!("{public_url}/covers/{h}-l.jpg"),
            });
        let authors: Vec<String> = authors_o
            .and_then(|a| serde_json::from_str(&a).ok())
            .unwrap_or_else(|| {
                work.people
                    .iter()
                    .filter(|p| p.role == "author")
                    .map(|p| p.name.clone())
                    .collect()
            });
        let book = Book {
            title: title_o.unwrap_or_else(|| work.title.clone()),
            authors,
            people: work.people.clone(),
            first_year: work.first_year,
            cover,
            editions: work
                .editions
                .iter()
                .map(|e| Edition {
                    id: e.id.clone(),
                    isbn13: e.isbn13.clone(),
                    publisher: e.publisher.clone(),
                    binding: e.binding.clone(),
                    year: e.year,
                    pages: e.pages,
                })
                .collect(),
            state: state_name,
            added_at,
            tags: tags.remove(&id).unwrap_or_default(),
            rating: auth
                .allows("ratings:read")
                .then_some((rating > 0).then_some(rating)),
            notes: auth.allows("notes:read").then_some(notes),
            copies: auth
                .allows("copies:read")
                .then(|| copies.remove(&id).unwrap_or_default()),
            reading: auth
                .allows("reading:read")
                .then(|| current.map(reading_from)),
            work_id,
            id: id.clone(),
        };
        by_id.insert(id, book);
    }
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn one_book(
    conn: &mut SqliteConnection,
    state: &AppState,
    auth: &TokenAuth,
    id: &str,
) -> ApiResult<Book> {
    load_books(conn, state, auth, &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(no_book)
}

/// What a change answers: the book, if the token may see books, else just ok.
async fn answer_book(
    state: &AppState,
    auth: &TokenAuth,
    id: &str,
    status: StatusCode,
) -> ApiResult<Response> {
    if !auth.allows("books:read") {
        return Ok((status, Json(json!({ "ok": true }))).into_response());
    }
    let mut conn = state.db.acquire().await?;
    let b = one_book(&mut conn, state, auth, id).await?;
    Ok((status, Json(b)).into_response())
}

// ---------------------------------------------------------------------------
// Profile

async fn profile(auth: TokenAuth) -> ApiResult<Json<Profile>> {
    auth.require("profile:read")?;
    let u = &auth.user;
    Ok(Json(Profile {
        id: u.id.clone(),
        username: u.username.clone(),
        display_name: u.display_name.clone(),
        bio: u.bio.clone(),
        time_zone: u.time_zone.clone(),
    }))
}

// ---------------------------------------------------------------------------
// Books

#[derive(Deserialize)]
struct ListQuery {
    /// active, inactive, or finished
    state: Option<String>,
    /// The `next` of the previous page.
    after: Option<String>,
    /// 1–200, default 50.
    limit: Option<i64>,
}

/// SQL for "the book's latest reading is finished".
const FINISHED: &str = "EXISTS (SELECT 1 FROM readings r WHERE r.book_id = b.id AND r.deleted = 0
    AND r.finished_on IS NOT NULL
    AND r.id = (SELECT MAX(x.id) FROM readings x WHERE x.book_id = b.id AND x.deleted = 0))";

async fn list_books(
    State(state): State<AppState>,
    auth: TokenAuth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    auth.require("books:read")?;
    let filter = match q.state.as_deref() {
        None | Some("") => "",
        Some("finished") => FINISHED,
        Some("active") => "b.active = 1",
        Some("inactive") => "b.active = 0",
        Some(_) => {
            return Err(bad("state", "state is active, inactive, or finished."));
        }
    };
    let not_finished = matches!(q.state.as_deref(), Some("active" | "inactive"));
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let mut cond = String::from("b.user_id = ? AND b.deleted = 0 AND b.id > ?");
    if !filter.is_empty() {
        cond.push_str(" AND ");
        cond.push_str(filter);
    }
    if not_finished {
        cond.push_str(" AND NOT ");
        cond.push_str(FINISHED);
    }
    let mut conn = state.db.acquire().await?;
    // The SQL is put together from the constant pieces above only.
    let mut ids: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT b.id FROM books b WHERE {cond} ORDER BY b.id LIMIT ?"
    )))
    .bind(&auth.user.id)
    .bind(q.after.as_deref().unwrap_or(""))
    .bind(limit + 1)
    .fetch_all(&mut *conn)
    .await?;
    let next = if ids.len() as i64 > limit {
        ids.truncate(limit as usize);
        ids.last().cloned()
    } else {
        None
    };
    let books = load_books(&mut conn, &state, &auth, &ids).await?;
    Ok(Json(json!({ "books": books, "next": next })))
}

async fn get_book(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Book>> {
    auth.require("books:read")?;
    let mut conn = state.db.acquire().await?;
    Ok(Json(one_book(&mut conn, &state, &auth, &id).await?))
}

#[derive(Deserialize)]
struct CatalogRef {
    provider: String,
    source_id: String,
    isbn: Option<String>,
}

#[derive(Deserialize)]
struct ManualBook {
    title: String,
    authors: Vec<String>,
    first_year: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddBook {
    /// A work already in the catalog (shared, or your own manual entry).
    work_id: Option<String>,
    /// A catalog search result (its provider and source_id).
    catalog: Option<CatalogRef>,
    /// A book entered by hand, private to you.
    manual: Option<ManualBook>,
    /// An ISBN-10 or ISBN-13: found in the shared catalog, or else looked up
    /// the way a catalog search would (your Google Books key if you have one).
    isbn: Option<String>,
    /// Needs tags:write.
    tags: Option<Vec<String>>,
    /// A copy you own, added with the book. Needs copies:write. Without an
    /// `edition_id`, the edition with the ISBN you added by (if any).
    copy: Option<NewCopy>,
}

fn normalize_isbn(raw: &str) -> ApiResult<String> {
    mau_core::isbn::normalize(raw).ok_or_else(|| {
        bad(
            "isbn",
            "That isn't a valid ISBN. Check for a typo: the last digit is a check digit.",
        )
    })
}

/// The edition of a work with this ISBN that the user can see.
async fn edition_by_isbn(
    conn: &mut SqliteConnection,
    user_id: &str,
    work_id: &str,
    isbn: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT id FROM editions WHERE work_id = ? AND isbn13 = ? AND (owner_id IS NULL OR owner_id = ?) LIMIT 1",
    )
    .bind(work_id)
    .bind(isbn)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Checks a copy's `edition_id`: one of the book's own editions, or empty
/// for an unknown edition.
async fn check_edition(
    conn: &mut SqliteConnection,
    user_id: &str,
    book_id: &str,
    edition_id: Option<&str>,
) -> ApiResult<()> {
    match edition_id {
        Some(e) if !e.is_empty() => {
            if library::book_edition(conn, user_id, book_id, e)
                .await?
                .is_none()
            {
                return Err(bad(
                    "edition_id",
                    "That edition isn't one of this book's. Use an id from the book's editions.",
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Rank before the first inactive book (a new or paused book's place).
async fn top_of_inactive(
    conn: &mut SqliteConnection,
    user_id: &str,
    except: &str,
) -> sqlx::Result<String> {
    let first: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT MIN(b.rank) FROM books b WHERE b.user_id = ? AND b.deleted = 0 AND b.active = 0 AND b.id != ? AND NOT {FINISHED}"
    )))
    .bind(user_id)
    .bind(except)
    .fetch_one(conn)
    .await?;
    Ok(mau_core::rank::between(None, first.as_deref()))
}

/// Rank after the last active book (where a started or resumed book goes).
async fn end_of_active(
    conn: &mut SqliteConnection,
    user_id: &str,
    except: &str,
) -> sqlx::Result<String> {
    let last: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT MAX(b.rank) FROM books b WHERE b.user_id = ? AND b.deleted = 0 AND b.active = 1 AND b.id != ? AND NOT {FINISHED}"
    )))
    .bind(user_id)
    .bind(except)
    .fetch_one(conn)
    .await?;
    Ok(mau_core::rank::between(last.as_deref(), None))
}

async fn add_book(
    State(state): State<AppState>,
    auth: TokenAuth,
    Json(req): Json<AddBook>,
) -> ApiResult<Response> {
    auth.require("books:write")?;
    if req.tags.as_ref().is_some_and(|t| !t.is_empty()) {
        auth.require("tags:write")?;
    }
    if req.copy.is_some() {
        auth.require("copies:write")?;
    }
    let given = [
        req.work_id.is_some(),
        req.catalog.is_some(),
        req.manual.is_some(),
        req.isbn.is_some(),
    ];
    if given.iter().filter(|g| **g).count() != 1 {
        return Err(bad(
            "book",
            "Send one of work_id, catalog (a search result's provider and source_id), isbn, or manual.",
        ));
    }
    let user_id = auth.user.id.clone();
    // The ISBN the book was added by, for the copy's edition.
    let wanted_isbn = match (
        req.isbn.as_deref(),
        req.catalog.as_ref().and_then(|c| c.isbn.as_deref()),
    ) {
        (Some(i), _) | (None, Some(i)) => Some(normalize_isbn(i)?),
        _ => None,
    };
    // By ISBN: the shared catalog first; otherwise the providers, as a
    // search would ask them (and counted as one).
    let isbn_work = match (&req.isbn, &wanted_isbn) {
        (Some(_), Some(isbn)) => {
            let shared: Option<String> = sqlx::query_scalar(
                "SELECT work_id FROM editions WHERE owner_id IS NULL AND isbn13 = ? LIMIT 1",
            )
            .bind(isbn)
            .fetch_optional(&state.db)
            .await?;
            match shared {
                Some(w) => Some(w),
                None => {
                    state.limits.check("search", &user_id, 30)?;
                    let cand = crate::catalog::isbn_candidate(
                        &state,
                        auth.user.google_books_key.as_deref(),
                        isbn,
                    )
                    .await?
                    .ok_or_else(|| {
                        ApiError::not_found(format!(
                            "No book with ISBN {isbn} was found in the catalog or its sources. Add it by hand (manual) instead."
                        ))
                    })?;
                    Some(
                        crate::catalog::ensure_candidate(&state, &cand, Some(isbn))
                            .await?
                            .id,
                    )
                }
            }
        }
        _ => None,
    };
    // A catalog result goes into the shared catalog first (from the cached
    // search result only, so apps can't make up catalog records).
    let catalog_work = match &req.catalog {
        Some(c) => {
            let cand = crate::catalog::cached_candidate(&state, &c.provider, &c.source_id).await?;
            Some(
                crate::catalog::ensure_candidate(&state, &cand, c.isbn.as_deref())
                    .await?
                    .id,
            )
        }
        None => None,
    };
    let mut b = Batch::begin(&state).await?;
    let work_id = match (&req.work_id, catalog_work.or(isbn_work), &req.manual) {
        (Some(w), _, _) => w.clone(),
        (_, Some(w), _) => w,
        (_, _, Some(m)) => {
            let id = new_id();
            let people = m
                .authors
                .iter()
                .map(|a| Person {
                    name: a.clone(),
                    role: "author".into(),
                })
                .collect();
            b.apply(
                &state,
                &user_id,
                OpKind::WorkCreate {
                    work: NewWork {
                        id: id.clone(),
                        title: m.title.clone(),
                        first_year: m.first_year,
                        people,
                        edition: None,
                    },
                },
            )
            .await?;
            id
        }
        _ => unreachable!("one was given"),
    };
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM books WHERE user_id = ? AND work_id = ? AND deleted = 0",
    )
    .bind(&user_id)
    .bind(&work_id)
    .fetch_optional(b.conn())
    .await?;
    if let Some(existing) = existing {
        return Err(ApiError::conflict(
            "exists",
            format!("That book is already on your list ({existing})."),
        ));
    }
    let id = new_id();
    let rank = top_of_inactive(b.conn(), &user_id, "").await?;
    let applied = b
        .apply(
            &state,
            &user_id,
            OpKind::BookCreate {
                id: id.clone(),
                work_id: work_id.clone(),
                rank,
                added_at: now_ms(),
            },
        )
        .await?;
    if applied != Applied::Yes {
        return Err(bad("work_id", "No such work in the catalog."));
    }
    if let Some(copy) = req.copy {
        let edition_id = match (&copy.edition_id, &wanted_isbn) {
            (Some(e), _) => {
                check_edition(b.conn(), &user_id, &id, Some(e)).await?;
                Some(e.clone())
            }
            (None, Some(isbn)) => edition_by_isbn(b.conn(), &user_id, &work_id, isbn).await?,
            (None, None) => None,
        };
        b.apply(
            &state,
            &user_id,
            OpKind::CopyCreate {
                id: new_id(),
                book_id: id.clone(),
                format: copy.format,
                location: copy.location,
                edition_id,
            },
        )
        .await?;
    }
    for tag in req.tags.unwrap_or_default() {
        b.apply(
            &state,
            &user_id,
            OpKind::TagSet {
                book_id: id.clone(),
                tag,
                present: true,
            },
        )
        .await?;
    }
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::CREATED).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditBook {
    /// The title you see ("" goes back to the catalog's). books:write.
    title: Option<String>,
    /// The authors you see ([] goes back to the catalog's). books:write.
    authors: Option<Vec<String>>,
    /// 1–5 stars, 0 for none. ratings:write.
    rating: Option<i64>,
    /// ratings:write. notes:write.
    notes: Option<String>,
}

async fn live_book(conn: &mut SqliteConnection, user_id: &str, id: &str) -> ApiResult<()> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM books WHERE id = ? AND user_id = ? AND deleted = 0)",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    if ok { Ok(()) } else { Err(no_book()) }
}

async fn edit_book(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    Json(req): Json<EditBook>,
) -> ApiResult<Response> {
    if req.title.is_some() || req.authors.is_some() {
        auth.require("books:write")?;
    }
    if req.rating.is_some() {
        auth.require("ratings:write")?;
    }
    if req.notes.is_some() {
        auth.require("notes:write")?;
    }
    let mut b = Batch::begin(&state).await?;
    live_book(b.conn(), &auth.user.id, &id).await?;
    let fields = BookFields {
        title_override: req.title,
        authors_override: req.authors,
        rating: req.rating,
        notes: req.notes,
        ..Default::default()
    };
    b.apply(
        &state,
        &auth.user.id,
        OpKind::BookSet {
            id: id.clone(),
            fields,
        },
    )
    .await?;
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::OK).await
}

async fn remove_book(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    auth.require("books:delete")?;
    let mut b = Batch::begin(&state).await?;
    live_book(b.conn(), &auth.user.id, &id).await?;
    b.apply(&state, &auth.user.id, OpKind::BookDelete { id })
        .await?;
    b.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn set_tag(
    state: AppState,
    auth: TokenAuth,
    id: String,
    tag: String,
    present: bool,
) -> ApiResult<Json<Value>> {
    auth.require("tags:write")?;
    let mut b = Batch::begin(&state).await?;
    live_book(b.conn(), &auth.user.id, &id).await?;
    b.apply(
        &state,
        &auth.user.id,
        OpKind::TagSet {
            book_id: id,
            tag,
            present,
        },
    )
    .await?;
    b.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn add_tag(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path((id, tag)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    set_tag(state, auth, id, tag, true).await
}

async fn remove_tag(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path((id, tag)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    set_tag(state, auth, id, tag, false).await
}

// ---------------------------------------------------------------------------
// Copies

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewCopy {
    /// Hardcover, Paperback, Ebook, or Audiobook.
    format: String,
    #[serde(default)]
    location: String,
    edition_id: Option<String>,
}

async fn copy_answer(state: &AppState, user_id: &str, id: &str) -> ApiResult<Json<Copy>> {
    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT id, format, location, edition_id FROM copies WHERE id = ? AND user_id = ? AND deleted = 0",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("No such copy."))?;
    Ok(Json(Copy {
        id: row.0,
        format: row.1,
        location: row.2,
        edition_id: row.3,
    }))
}

async fn add_copy(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(book_id): Path<String>,
    Json(req): Json<NewCopy>,
) -> ApiResult<(StatusCode, Json<Copy>)> {
    auth.require("copies:write")?;
    let mut b = Batch::begin(&state).await?;
    live_book(b.conn(), &auth.user.id, &book_id).await?;
    check_edition(b.conn(), &auth.user.id, &book_id, req.edition_id.as_deref()).await?;
    let id = new_id();
    b.apply(
        &state,
        &auth.user.id,
        OpKind::CopyCreate {
            id: id.clone(),
            book_id,
            format: req.format,
            location: req.location,
            edition_id: req.edition_id,
        },
    )
    .await?;
    b.commit().await?;
    Ok((
        StatusCode::CREATED,
        copy_answer(&state, &auth.user.id, &id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditCopy {
    format: Option<String>,
    location: Option<String>,
    /// "" for an unknown edition.
    edition_id: Option<String>,
}

async fn live_copy(conn: &mut SqliteConnection, user_id: &str, id: &str) -> ApiResult<()> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM copies WHERE id = ? AND user_id = ? AND deleted = 0)",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    if ok {
        Ok(())
    } else {
        Err(ApiError::not_found("No such copy."))
    }
}

async fn edit_copy(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    Json(req): Json<EditCopy>,
) -> ApiResult<Json<Copy>> {
    auth.require("copies:write")?;
    let mut b = Batch::begin(&state).await?;
    live_copy(b.conn(), &auth.user.id, &id).await?;
    let book_id: String = sqlx::query_scalar("SELECT book_id FROM copies WHERE id = ?")
        .bind(&id)
        .fetch_one(b.conn())
        .await?;
    check_edition(b.conn(), &auth.user.id, &book_id, req.edition_id.as_deref()).await?;
    b.apply(
        &state,
        &auth.user.id,
        OpKind::CopySet {
            id: id.clone(),
            fields: CopyFields {
                format: req.format,
                location: req.location,
                edition_id: req.edition_id,
            },
        },
    )
    .await?;
    b.commit().await?;
    copy_answer(&state, &auth.user.id, &id).await
}

async fn remove_copy(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    auth.require("copies:write")?;
    let mut b = Batch::begin(&state).await?;
    live_copy(b.conn(), &auth.user.id, &id).await?;
    b.apply(&state, &auth.user.id, OpKind::CopyDelete { id })
        .await?;
    b.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Reading

async fn readings(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    auth.require("reading:read")?;
    let mut conn = state.db.acquire().await?;
    live_book(&mut conn, &auth.user.id, &id).await?;
    let ids_json = serde_json::to_string(&[&id]).expect("strings serialize");
    let list = readings_of(&mut conn, &auth.user.id, &ids_json)
        .await?
        .remove(&id)
        .unwrap_or_default();
    let mut out = Vec::with_capacity(list.len());
    for r in &list {
        let events: Vec<(String, i64, i64, String)> = sqlx::query_as(
            "SELECT local_date, from_page, to_page, unit FROM progress_events
             WHERE reading_id = ? AND deleted = 0 ORDER BY hlc",
        )
        .bind(&r.0)
        .fetch_all(&mut *conn)
        .await?;
        out.push(ReadingWithEvents {
            reading: reading_from(r),
            events: events
                .into_iter()
                .map(|(date, from, to, unit)| Event {
                    date,
                    from,
                    to,
                    unit: Unit::parse(&unit).as_str(),
                })
                .collect(),
        });
    }
    Ok(Json(json!({ "readings": out })))
}

/// What the reading actions need to know about a book.
struct Status {
    active: bool,
    /// The latest reading.
    current: Option<ReadingTuple>,
    /// A page count from the catalog's editions, if one lists it.
    catalog_pages: Option<i64>,
}

impl Status {
    fn finished(&self) -> bool {
        self.current.as_ref().is_some_and(|r| r.3.is_some())
    }
}

async fn status(conn: &mut SqliteConnection, user_id: &str, id: &str) -> ApiResult<Status> {
    let row: Option<(bool, String)> = sqlx::query_as(
        "SELECT active, work_id FROM books WHERE id = ? AND user_id = ? AND deleted = 0",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (active, work_id) = row.ok_or_else(no_book)?;
    let ids_json = serde_json::to_string(&[id]).expect("strings serialize");
    let current = readings_of(conn, user_id, &ids_json)
        .await?
        .remove(id)
        .and_then(|mut v| v.pop());
    let catalog_pages: Option<i64> = sqlx::query_scalar(
        "SELECT pages FROM editions WHERE work_id = ? AND pages IS NOT NULL AND (owner_id IS NULL OR owner_id = ?) ORDER BY rowid LIMIT 1",
    )
    .bind(&work_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Status {
        active,
        current,
        catalog_pages,
    })
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    /// pages (default) or percent.
    unit: Option<String>,
    /// Pages: the copy's page count. Defaults to the last reading's, or the
    /// catalog's; without either, the reading counts by percent.
    total: Option<i64>,
    /// Percent: the copy's page count, if known.
    pages: Option<i64>,
    /// YYYY-MM-DD; default today in the reader's time zone.
    date: Option<String>,
}

/// Starts reading: resumes a started book, starts a never-started one, or
/// starts a re-read of a finished one. The book becomes active, after the
/// other active books.
async fn start(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    body: Option<Json<StartRequest>>,
) -> ApiResult<Response> {
    auth.require("reading:write")?;
    let req = body.map(|Json(r)| r).unwrap_or_default();
    let day = date_or_today(req.date.as_deref(), &auth.user.time_zone)?;
    let user_id = auth.user.id.clone();
    let mut b = Batch::begin(&state).await?;
    let s = status(b.conn(), &user_id, &id).await?;
    let resuming = s.current.is_some() && !s.finished();
    if resuming && s.active {
        b.commit().await?;
        return answer_book(&state, &auth, &id, StatusCode::OK).await;
    }
    if !resuming {
        let last = s.current.as_ref();
        let percent = match req.unit.as_deref() {
            Some("percent") => true,
            Some("pages") => false,
            Some(_) => return Err(bad("unit", "unit is pages or percent.")),
            None => {
                req.total.is_none()
                    && match last {
                        Some(r) => Unit::parse(&r.4) == Unit::Percent,
                        None => s.catalog_pages.is_none(),
                    }
            }
        };
        let kind = if percent {
            OpKind::ReadingCreate {
                id: new_id(),
                book_id: id.clone(),
                started_on: day,
                total: 100,
                unit: Unit::Percent,
                pages: req
                    .pages
                    .or_else(|| last.and_then(|r| r.7))
                    .or(s.catalog_pages),
            }
        } else {
            let total = req
                .total
                .or_else(|| {
                    last.filter(|r| Unit::parse(&r.4) == Unit::Pages)
                        .map(|r| r.5)
                })
                .or(s.catalog_pages)
                .ok_or_else(|| {
                    bad(
                        "total",
                        "How many pages does the copy have? Send total, or unit: percent.",
                    )
                })?;
            OpKind::ReadingCreate {
                id: new_id(),
                book_id: id.clone(),
                started_on: day,
                total,
                unit: Unit::Pages,
                pages: None,
            }
        };
        b.apply(&state, &user_id, kind).await?;
    }
    let rank = end_of_active(b.conn(), &user_id, &id).await?;
    b.apply(
        &state,
        &user_id,
        OpKind::BookSet {
            id: id.clone(),
            fields: BookFields {
                active: Some(true),
                rank: Some(rank),
                ..Default::default()
            },
        },
    )
    .await?;
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::OK).await
}

/// Pauses an active book: inactive, at the top of the inactive books.
async fn pause(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    auth.require("reading:write")?;
    let user_id = auth.user.id.clone();
    let mut b = Batch::begin(&state).await?;
    let s = status(b.conn(), &user_id, &id).await?;
    if s.active && !s.finished() {
        let rank = top_of_inactive(b.conn(), &user_id, &id).await?;
        b.apply(
            &state,
            &user_id,
            OpKind::BookSet {
                id: id.clone(),
                fields: BookFields {
                    active: Some(false),
                    rank: Some(rank),
                    ..Default::default()
                },
            },
        )
        .await?;
    }
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::OK).await
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DateRequest {
    date: Option<String>,
}

/// Marks a book finished (today, or `date`). The position stays where it
/// was, so nothing counts as read that day. A never-started book gets a
/// reading started and finished that day.
async fn finish(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    body: Option<Json<DateRequest>>,
) -> ApiResult<Response> {
    auth.require("reading:write")?;
    let req = body.map(|Json(r)| r).unwrap_or_default();
    let day = date_or_today(req.date.as_deref(), &auth.user.time_zone)?;
    let user_id = auth.user.id.clone();
    let mut b = Batch::begin(&state).await?;
    let s = status(b.conn(), &user_id, &id).await?;
    if !s.finished() {
        let reading_id = match &s.current {
            Some(r) => r.0.clone(),
            None => {
                let rid = new_id();
                let kind = match s.catalog_pages {
                    Some(total) => OpKind::ReadingCreate {
                        id: rid.clone(),
                        book_id: id.clone(),
                        started_on: day.clone(),
                        total,
                        unit: Unit::Pages,
                        pages: None,
                    },
                    None => OpKind::ReadingCreate {
                        id: rid.clone(),
                        book_id: id.clone(),
                        started_on: day.clone(),
                        total: 100,
                        unit: Unit::Percent,
                        pages: None,
                    },
                };
                b.apply(&state, &user_id, kind).await?;
                rid
            }
        };
        b.apply(
            &state,
            &user_id,
            OpKind::ReadingSet {
                id: reading_id,
                fields: ReadingFields {
                    finished_on: Some(day),
                    ..Default::default()
                },
            },
        )
        .await?;
        if s.active {
            b.apply(
                &state,
                &user_id,
                OpKind::BookSet {
                    id: id.clone(),
                    fields: BookFields {
                        active: Some(false),
                        ..Default::default()
                    },
                },
            )
            .await?;
        }
    }
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressRequest {
    /// The new position: a page, or a percent for a percent reading.
    position: Option<i64>,
    /// Or pages (percent) to add; negative moves back.
    add: Option<i64>,
    /// Pages readings: a new page count for the copy.
    total: Option<i64>,
    /// The day it was read, YYYY-MM-DD; default today in the reader's time zone.
    date: Option<String>,
}

/// Moves the current reading's position (pages read that day count, as when
/// the reader moves it in the app). Reaching the total finishes the book.
async fn progress(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    Json(req): Json<ProgressRequest>,
) -> ApiResult<Response> {
    auth.require("reading:write")?;
    let day = date_or_today(req.date.as_deref(), &auth.user.time_zone)?;
    let user_id = auth.user.id.clone();
    let mut b = Batch::begin(&state).await?;
    let s = status(b.conn(), &user_id, &id).await?;
    let Some(r) = s.current.clone().filter(|r| r.3.is_none()) else {
        return Err(ApiError::conflict(
            "not_reading",
            "Start the book first (POST /api/v1/books/{id}/start).",
        ));
    };
    let (reading_id, _, _, _, unit, total, pos, _, pos_clock) = r;
    let unit = Unit::parse(&unit);
    let new_total = match (unit, req.total) {
        (Unit::Percent, Some(_)) => {
            return Err(bad("total", "A percent reading's total is always 100."));
        }
        (_, Some(t)) => t,
        (_, None) => total,
    };
    let to = match (req.position, req.add) {
        (Some(p), None) => p,
        (None, Some(n)) => pos.saturating_add(n),
        (None, None) if req.total.is_some() => pos,
        _ => return Err(bad("position", "Send position or add (not both).")),
    }
    .clamp(0, new_total.max(0));
    if new_total != total {
        b.apply(
            &state,
            &user_id,
            OpKind::ReadingSet {
                id: reading_id.clone(),
                fields: ReadingFields {
                    total: Some(new_total),
                    ..Default::default()
                },
            },
        )
        .await?;
    }
    if to != pos {
        b.apply(
            &state,
            &user_id,
            OpKind::Progress {
                event_id: new_id(),
                reading_id: reading_id.clone(),
                base: pos_clock,
                from: pos,
                to,
                local_date: day.clone(),
                unit,
            },
        )
        .await?;
    }
    if to >= new_total {
        b.apply(
            &state,
            &user_id,
            OpKind::ReadingSet {
                id: reading_id,
                fields: ReadingFields {
                    finished_on: Some(day),
                    ..Default::default()
                },
            },
        )
        .await?;
        b.apply(
            &state,
            &user_id,
            OpKind::BookSet {
                id: id.clone(),
                fields: BookFields {
                    active: Some(false),
                    ..Default::default()
                },
            },
        )
        .await?;
    }
    b.commit().await?;
    answer_book(&state, &auth, &id, StatusCode::OK).await
}

// ---------------------------------------------------------------------------
// Catalog search and shares: the app's own handlers, as the token's user
// (a session that never confirmed a password).

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState>,
    auth: TokenAuth,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Response> {
    auth.require("catalog:search")?;
    let query = axum::extract::Query(
        serde_json::from_value(json!({ "q": q.q })).map_err(ApiError::internal)?,
    );
    Ok(
        crate::catalog::search(State(state), Authed(auth.session()), query)
            .await?
            .into_response(),
    )
}

async fn shares_list(State(state): State<AppState>, auth: TokenAuth) -> ApiResult<Response> {
    auth.require("shares:read")?;
    Ok(crate::shares::list(State(state), Authed(auth.session()))
        .await?
        .into_response())
}

async fn shares_create(
    State(state): State<AppState>,
    auth: TokenAuth,
    Json(req): Json<crate::shares::NewShare>,
) -> ApiResult<Response> {
    auth.require("shares:write")?;
    let r = crate::shares::create(State(state), Authed(auth.session()), Json(req)).await?;
    Ok((StatusCode::CREATED, r).into_response())
}

async fn shares_update(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
    Json(req): Json<crate::shares::ShareUpdate>,
) -> ApiResult<Response> {
    auth.require("shares:write")?;
    Ok(
        crate::shares::update(State(state), Authed(auth.session()), Path(id), Json(req))
            .await?
            .into_response(),
    )
}

async fn shares_delete(
    State(state): State<AppState>,
    auth: TokenAuth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    auth.require("shares:write")?;
    Ok(
        crate::shares::delete(State(state), Authed(auth.session()), Path(id))
            .await?
            .into_response(),
    )
}
