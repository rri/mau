//! Importing from other services, with review before anything changes.
//!
//! 1. `POST /api/import` parses the file and starts matching each row to the
//!    catalog in the background.
//! 2. `GET /api/import/{job}` reports progress, then each row's match:
//!    matched, uncertain (choose a candidate), unmatched (add as a manual
//!    entry or skip), or already on the list (skipped).
//!    `POST /api/import/{job}/rows/{index}` searches again with a corrected
//!    title and authors.
//! 3. `POST /api/import/{job}/commit` starts creating the books in the
//!    background; `GET /api/import/{job}` reports progress, then the result.
//!
//! Searches go to Google Books when the user has their own key, otherwise to
//! Open Library: title + first author, then, without a clear match, the
//! short title with punctuation removed + the author's surname.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, State};
use futures::StreamExt;
use mau_core::import::{self, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppState;
use crate::auth::session::Authed;
use crate::catalog;
use crate::error::{ApiError, ApiResult};
use crate::export;
use crate::library::{self, Person};
use crate::providers::{self, Candidate, ProviderConfig, ProviderName};
use crate::util::{HOUR_MS, new_id, now_ms};

/// Stands for "finished, date unknown" (spec: undated reads are finished).
pub const UNKNOWN_DATE: &str = "0000-01-01";
const MAX_ROWS: usize = 5000;
const CONCURRENCY: usize = 3;
/// Choices offered for a book without a clear match.
const MAX_CANDIDATES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MatchKind {
    Matched,
    Uncertain,
    Unmatched,
    Dupe,
}

#[derive(Debug, Clone)]
struct Matched {
    kind: MatchKind,
    candidates: Vec<Candidate>,
    /// An existing shared work (mau exports matched by ISBN).
    local_work: Option<String>,
    /// Why Google Books refused, when Open Library was asked instead.
    refused: Option<String>,
}

impl Matched {
    fn of(kind: MatchKind) -> Self {
        Matched {
            kind,
            candidates: vec![],
            local_work: None,
            refused: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Saving {
    Running { done: usize, total: usize },
    Done(CommitResult),
    Failed(String),
}

struct Job {
    user_id: String,
    rows: Vec<Row>,
    /// Full history for mau's own exports.
    mau: Option<Vec<export::Book>>,
    /// Search titles without trailing (…) notes from the start (dinky.dev).
    strip_notes: bool,
    /// The user's own Google Books key when the import started.
    google_key: Option<String>,
    results: Vec<Option<Matched>>,
    saving: Option<Saving>,
    created_at: i64,
}

impl Job {
    fn matched(&self) -> bool {
        self.results.iter().all(Option::is_some)
    }

    fn saving_now(&self) -> bool {
        matches!(self.saving, Some(Saving::Running { .. }))
    }
}

#[derive(Default)]
pub struct Jobs {
    inner: Mutex<HashMap<String, Arc<Mutex<Job>>>>,
}

impl Jobs {
    fn insert(&self, job: Job) -> String {
        let id = new_id();
        let mut map = self.inner.lock().unwrap();
        let cutoff = now_ms() - 6 * HOUR_MS;
        map.retain(|_, j| {
            let j = j.lock().unwrap();
            j.created_at > cutoff || j.saving_now()
        });
        map.insert(id.clone(), Arc::new(Mutex::new(job)));
        id
    }

    fn get(&self, id: &str, user_id: &str) -> Option<Arc<Mutex<Job>>> {
        let map = self.inner.lock().unwrap();
        map.get(id)
            .filter(|j| j.lock().unwrap().user_id == user_id)
            .cloned()
    }

    fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }

    /// The user's newest import, to pick up where they left off.
    fn latest(&self, user_id: &str) -> Option<String> {
        let map = self.inner.lock().unwrap();
        map.iter()
            .filter_map(|(id, j)| {
                let j = j.lock().unwrap();
                (j.user_id == user_id).then(|| (j.created_at, id.clone()))
            })
            .max()
            .map(|(_, id)| id)
    }
}

// ---------------------------------------------------------------------------
// Parsing

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct PreviewRequest {
    pub text: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CsvPreview {
    pub headers: Vec<String>,
    pub samples: Vec<Vec<String>>,
    /// mau fields a column can map to.
    pub fields: Vec<String>,
}

/// `POST /api/import/preview`: columns and sample rows of a CSV for mapping.
pub async fn preview(
    Authed(_): Authed,
    Json(req): Json<PreviewRequest>,
) -> ApiResult<Json<CsvPreview>> {
    let (headers, samples) = import::csv_preview(&req.text)
        .map_err(|e| ApiError::bad_request("format", e.to_string()))?;
    Ok(Json(CsvPreview {
        headers,
        samples,
        fields: import::FIELDS.iter().map(|f| f.to_string()).collect(),
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ImportStart {
    /// goodreads, storygraph, librarything, dinky, csv, or mau
    pub source: String,
    pub text: String,
    /// For `csv`: mau field → column name.
    pub mapping: Option<HashMap<String, String>>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ImportStarted {
    pub job: String,
    pub total: usize,
}

fn rows_from_mau(text: &str) -> ApiResult<(Vec<Row>, Vec<export::Book>)> {
    let e: export::Export = serde_json::from_str(text)
        .map_err(|e| ApiError::bad_request("format", format!("This isn't a mau export ({e}).")))?;
    if e.format != export::FORMAT {
        return Err(ApiError::bad_request("format", "This isn't a mau export."));
    }
    let rows = e
        .books
        .iter()
        .map(|b| Row {
            title: b.title.clone(),
            authors: b
                .people
                .iter()
                .filter(|p| p.role == "author")
                .map(|p| p.name.clone())
                .collect(),
            isbn13: b.editions.iter().find_map(|x| x.isbn13.clone()),
            pages: b.readings.last().map(|r| r.total),
            year: b.first_year,
            rating: b.rating,
            tags: b.tags.clone(),
            owned: !b.copies.is_empty(),
            notes: b.notes.clone(),
            ..Default::default()
        })
        .collect();
    Ok((rows, e.books))
}

pub async fn start(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<ImportStart>,
) -> ApiResult<Json<ImportStarted>> {
    state.limits.check("import", &s.user.id, 5)?;
    let bad = |e: import::Error| ApiError::bad_request("format", e.to_string());
    let (rows, mau) = match req.source.as_str() {
        "goodreads" => (import::goodreads(&req.text).map_err(bad)?, None),
        "storygraph" => (import::storygraph(&req.text).map_err(bad)?, None),
        "librarything" => (import::librarything(&req.text).map_err(bad)?, None),
        "dinky" => (import::dinky(&req.text).map_err(bad)?, None),
        "csv" => (
            import::generic(&req.text, &req.mapping.unwrap_or_default()).map_err(bad)?,
            None,
        ),
        "mau" => {
            let (rows, books) = rows_from_mau(&req.text)?;
            (rows, Some(books))
        }
        _ => return Err(ApiError::bad_request("source", "Unknown import source.")),
    };
    if rows.is_empty() {
        return Err(ApiError::bad_request(
            "empty",
            "No books were found in that file.",
        ));
    }
    if rows.len() > MAX_ROWS {
        return Err(ApiError::bad_request(
            "too_many",
            format!("Import at most {MAX_ROWS} books at a time."),
        ));
    }
    let total = rows.len();
    let job = state.imports.insert(Job {
        user_id: s.user.id.clone(),
        results: vec![None; total],
        rows,
        mau,
        strip_notes: req.source == "dinky",
        google_key: s.user.google_books_key.clone(),
        saving: None,
        created_at: now_ms(),
    });
    let handle = state.imports.get(&job, &s.user.id).expect("just inserted");
    tokio::spawn(match_all(state.clone(), s.user.id.clone(), handle));
    Ok(Json(ImportStarted { job, total }))
}

// ---------------------------------------------------------------------------
// Matching

/// A title for comparing: no trailing (…) notes, no subtitle, no leading
/// article, only letters and digits, lowercase.
fn norm_title(t: &str) -> String {
    let t = import::without_notes(t);
    let t = t.split(':').next().unwrap_or(t).to_lowercase();
    let t = ["the ", "a ", "an "]
        .iter()
        .find_map(|a| t.strip_prefix(a))
        .unwrap_or(&t)
        .to_string();
    t.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The surname as written: the last word before any comma, so "Mike Piper,
/// CPA" and "Bird, Richard" give Piper and Bird.
fn surname(a: &str) -> &str {
    let before = a.split(',').next().unwrap_or(a);
    before.split_whitespace().last().unwrap_or("")
}

fn norm_author(a: &str) -> String {
    surname(a)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Words only: apostrophes dropped ("Don't" → "Dont"), other punctuation
/// becomes a space. Open Library finds nothing for some hyphenated or
/// apostrophized titles when an author is in the query.
fn plain(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\'' | '’' | '‘' | '`'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The second search: the short title (no subtitle or trailing notes) and the
/// author's surname, punctuation removed. Open Library needs every word of a
/// query to be in a record, so middle initials, "CPA", or a different
/// subtitle make the first search come back empty.
fn fallback_query(title: &str, author: Option<&str>) -> String {
    let t = import::without_notes(title);
    let short = t.split(':').next().unwrap_or(t);
    format!("{} {}", plain(short), plain(author.map_or("", surname)))
        .trim()
        .to_string()
}

/// A clear match: the same title and one of the authors has the surname.
fn is_match(c: &Candidate, row: &Row) -> bool {
    norm_title(&c.title) == norm_title(&row.title)
        && row
            .authors
            .first()
            .is_none_or(|a| c.authors.iter().any(|x| norm_author(x) == norm_author(a)))
}

/// Where an import's searches go.
#[derive(Clone)]
struct Searcher {
    google: Option<ProviderConfig>,
    /// Unless an admin has turned it off.
    openlibrary: Option<ProviderConfig>,
}

impl Searcher {
    async fn new(state: &AppState, google_key: Option<&str>) -> Searcher {
        let openlibrary = providers::enabled(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.name == ProviderName::Openlibrary);
        Searcher {
            google: google_key.map(providers::own_google),
            openlibrary,
        }
    }

    async fn openlibrary_search(&self, state: &AppState, q: &str) -> Vec<Candidate> {
        let Some(p) = &self.openlibrary else {
            return vec![];
        };
        catalog::provider_search(state, p, q)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("import search failed: {e}");
                vec![]
            })
    }

    /// Results, and why Google refused if Open Library answered instead.
    async fn search(&self, state: &AppState, q: &str) -> (Vec<Candidate>, Option<String>) {
        if let Some(g) = &self.google {
            match catalog::provider_search(state, g, q).await {
                Ok(found) => return (found, None),
                Err(e) => return (self.openlibrary_search(state, q).await, Some(e)),
            }
        }
        (self.openlibrary_search(state, q).await, None)
    }

    async fn isbn(&self, state: &AppState, isbn: &str) -> (Option<Candidate>, Option<String>) {
        let mut refused = None;
        if let Some(g) = &self.google {
            match catalog::provider_isbn(state, g, isbn).await {
                Ok(found) => return (found, None),
                Err(e) => refused = Some(e),
            }
        }
        let found = match &self.openlibrary {
            Some(p) => catalog::provider_isbn(state, p, isbn)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("import ISBN lookup failed: {e}");
                    None
                }),
            None => None,
        };
        (found, refused)
    }
}

/// What the user already has, for spotting duplicates.
struct Existing {
    isbns: HashSet<String>,
    titles: HashSet<(String, String)>,
}

async fn existing(state: &AppState, user_id: &str) -> sqlx::Result<Existing> {
    let isbns: Vec<String> = sqlx::query_scalar(
        "SELECT e.isbn13 FROM editions e JOIN books b ON b.work_id = e.work_id
         WHERE b.user_id = ? AND b.deleted = 0 AND e.isbn13 IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let titles: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT w.title, (SELECT name FROM work_people p WHERE p.work_id = w.id AND p.role = 'author' ORDER BY position LIMIT 1)
         FROM works w JOIN books b ON b.work_id = w.id WHERE b.user_id = ? AND b.deleted = 0",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Existing {
        isbns: isbns.into_iter().collect(),
        titles: titles
            .into_iter()
            .map(|(t, a)| (norm_title(&t), norm_author(&a.unwrap_or_default())))
            .collect(),
    })
}

/// `first_title` is what the first search uses: the row's title, or for
/// dinky.dev the title without trailing notes.
async fn match_row(
    state: &AppState,
    ex: &Existing,
    searcher: &Searcher,
    row: &Row,
    first_title: &str,
    mau: bool,
) -> Matched {
    let dupe = row.isbn13.as_ref().is_some_and(|i| ex.isbns.contains(i))
        || ex.titles.contains(&(
            norm_title(&row.title),
            norm_author(row.authors.first().map_or("", |s| s)),
        ));
    if dupe {
        return Matched::of(MatchKind::Dupe);
    }
    // Something already in the shared catalog with this ISBN.
    if let Some(isbn) = &row.isbn13 {
        let local: Option<String> = sqlx::query_scalar(
            "SELECT work_id FROM editions WHERE owner_id IS NULL AND isbn13 = ? LIMIT 1",
        )
        .bind(isbn)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        if local.is_some() {
            return Matched {
                local_work: local,
                ..Matched::of(MatchKind::Matched)
            };
        }
    }
    if mau {
        // A mau export carries everything needed; don't ask providers.
        return Matched::of(MatchKind::Unmatched);
    }
    find(state, searcher, row, first_title, true).await
}

/// Asks the catalog: by ISBN (if `by_isbn`), then title + first author, then
/// the fallback search. Without a clear match, offers what both searches
/// found.
async fn find(
    state: &AppState,
    searcher: &Searcher,
    row: &Row,
    first_title: &str,
    by_isbn: bool,
) -> Matched {
    let mut refused = None;
    if by_isbn && let Some(isbn) = &row.isbn13 {
        let (found, r) = searcher.isbn(state, isbn).await;
        refused = refused.or(r);
        if let Some(c) = found {
            return Matched {
                candidates: vec![c],
                refused,
                ..Matched::of(MatchKind::Matched)
            };
        }
    }
    let author = row.authors.first().map(String::as_str);
    let first = format!("{} {}", first_title, author.unwrap_or(""))
        .trim()
        .to_string();
    let (mut found, r) = searcher.search(state, &first).await;
    refused = refused.or(r);
    if let Some(c) = found.iter().find(|c| is_match(c, row)) {
        return Matched {
            candidates: vec![c.clone()],
            refused,
            ..Matched::of(MatchKind::Matched)
        };
    }
    let second = fallback_query(&row.title, author);
    if !second.is_empty() && !second.eq_ignore_ascii_case(&first) {
        let (more, r) = searcher.search(state, &second).await;
        refused = refused.or(r);
        if let Some(c) = more.iter().find(|c| is_match(c, row)) {
            return Matched {
                candidates: vec![c.clone()],
                refused,
                ..Matched::of(MatchKind::Matched)
            };
        }
        for c in more {
            if !found
                .iter()
                .any(|f| f.provider == c.provider && f.source_id == c.source_id)
            {
                found.push(c);
            }
        }
    }
    found.truncate(MAX_CANDIDATES);
    let kind = if found.is_empty() {
        MatchKind::Unmatched
    } else {
        MatchKind::Uncertain
    };
    Matched {
        candidates: found,
        refused,
        ..Matched::of(kind)
    }
}

async fn match_all(state: AppState, user_id: String, job: Arc<Mutex<Job>>) {
    let (rows, mau, strip, google_key) = {
        let j = job.lock().unwrap();
        (
            j.rows.clone(),
            j.mau.is_some(),
            j.strip_notes,
            j.google_key.clone(),
        )
    };
    let ex = match existing(&state, &user_id).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("import: {e}");
            Existing {
                isbns: HashSet::new(),
                titles: HashSet::new(),
            }
        }
    };
    let ex = Arc::new(ex);
    let searcher = Searcher::new(&state, google_key.as_deref()).await;
    futures::stream::iter(rows.into_iter().enumerate())
        .map(|(i, row)| {
            let (state, ex, searcher) = (state.clone(), ex.clone(), searcher.clone());
            async move {
                let title = if strip {
                    import::without_notes(&row.title)
                } else {
                    &row.title
                };
                (i, match_row(&state, &ex, &searcher, &row, title, mau).await)
            }
        })
        .buffer_unordered(CONCURRENCY)
        .for_each(|(i, m)| {
            job.lock().unwrap().results[i] = Some(m);
            async {}
        })
        .await;
}

// ---------------------------------------------------------------------------
// Status

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CandidateBrief {
    pub title: String,
    pub authors: Vec<String>,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RowStatus {
    pub index: usize,
    pub title: String,
    pub authors: Vec<String>,
    /// e.g. "read 2026-10-20", "read (date unknown)", "not finished"
    pub summary: String,
    pub tags: Vec<String>,
    pub kind: MatchKind,
    pub candidates: Vec<CandidateBrief>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SaveProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct JobStatus {
    pub total: usize,
    pub done: usize,
    /// Present once every row is matched, until saving starts.
    pub rows: Option<Vec<RowStatus>>,
    /// Books searched on Open Library because Google Books refused.
    pub refused: usize,
    /// Google's reason, from the latest refusal.
    pub refused_reason: Option<String>,
    /// While saving.
    pub saving: Option<SaveProgress>,
    /// Once saved.
    pub result: Option<CommitResult>,
    /// If saving failed.
    pub error: Option<String>,
}

fn summary(row: &Row) -> String {
    match row.reads.last() {
        Some(r) => match &r.finished {
            Some(d) => format!("read {d}"),
            None => "read (date unknown)".into(),
        },
        None => "not finished → inactive".into(),
    }
}

fn row_status(index: usize, row: &Row, m: &Matched) -> RowStatus {
    RowStatus {
        index,
        title: row.title.clone(),
        authors: row.authors.clone(),
        summary: summary(row),
        tags: row.tags.clone(),
        kind: m.kind,
        candidates: m
            .candidates
            .iter()
            .map(|c| CandidateBrief {
                title: c.title.clone(),
                authors: c.authors.clone(),
                first_year: c.first_year,
            })
            .collect(),
    }
}

fn job_of(state: &AppState, id: &str, user_id: &str) -> ApiResult<Arc<Mutex<Job>>> {
    state
        .imports
        .get(id, user_id)
        .ok_or_else(|| ApiError::not_found("That import has expired. Start again."))
}

pub async fn status(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<JobStatus>> {
    let job = job_of(&state, &id, &s.user.id)?;
    let j = job.lock().unwrap();
    let done = j.results.iter().filter(|r| r.is_some()).count();
    let rows = (j.matched() && j.saving.is_none()).then(|| {
        j.rows
            .iter()
            .zip(&j.results)
            .enumerate()
            .map(|(index, (row, m))| row_status(index, row, m.as_ref().expect("all matched")))
            .collect()
    });
    let refusals: Vec<&String> = j
        .results
        .iter()
        .flatten()
        .filter_map(|m| m.refused.as_ref())
        .collect();
    let (saving, result, error) = match &j.saving {
        None => (None, None, None),
        Some(Saving::Running { done, total }) => (
            Some(SaveProgress {
                done: *done,
                total: *total,
            }),
            None,
            None,
        ),
        Some(Saving::Done(r)) => (None, Some(r.clone()), None),
        Some(Saving::Failed(e)) => (None, None, Some(e.clone())),
    };
    Ok(Json(JobStatus {
        total: j.rows.len(),
        done,
        rows,
        refused: refusals.len(),
        refused_reason: refusals.last().map(|r| r.to_string()),
        saving,
        result,
        error,
    }))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CurrentImport {
    pub job: Option<String>,
}

/// `GET /api/import/current`: the user's newest import, if one is still
/// kept, so the page can pick up where it was.
pub async fn current(State(state): State<AppState>, Authed(s): Authed) -> Json<CurrentImport> {
    Json(CurrentImport {
        job: state.imports.latest(&s.user.id),
    })
}

/// `DELETE /api/import/{job}`: drops an import that isn't being saved.
pub async fn dismiss(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let job = job_of(&state, &id, &s.user.id)?;
    if job.lock().unwrap().saving_now() {
        return Err(ApiError::conflict(
            "saving",
            "This import is being saved. Wait for it to finish.",
        ));
    }
    state.imports.remove(&id);
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RowEdit {
    pub title: String,
    pub authors: Vec<String>,
}

/// `POST /api/import/{job}/rows/{index}`: searches again with a corrected
/// title and authors, which a manual entry then uses too.
pub async fn research(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path((id, index)): Path<(String, usize)>,
    Json(req): Json<RowEdit>,
) -> ApiResult<Json<RowStatus>> {
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiError::bad_request("title", "Enter a title."));
    }
    let authors: Vec<String> = req
        .authors
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .take(20)
        .collect();
    let job = job_of(&state, &id, &s.user.id)?;
    let (mut row, google_key) = {
        let j = job.lock().unwrap();
        if !j.matched() || j.saving.is_some() {
            return Err(ApiError::conflict(
                "not_ready",
                "This import can't be changed now.",
            ));
        }
        let row = j
            .rows
            .get(index)
            .cloned()
            .ok_or_else(|| ApiError::not_found("No such row."))?;
        (row, j.google_key.clone())
    };
    state.limits.check("import-search", &s.user.id, 30)?;
    row.title = title.chars().take(300).collect();
    row.authors = authors;
    let searcher = Searcher::new(&state, google_key.as_deref()).await;
    let m = find(&state, &searcher, &row, &row.title, false).await;
    let out = row_status(index, &row, &m);
    let mut j = job.lock().unwrap();
    j.rows[index] = row;
    j.results[index] = Some(m);
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Commit

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct Choice {
    pub index: usize,
    /// match, manual, or skip
    pub action: String,
    /// Which candidate, for `match` (default 0).
    pub candidate: Option<usize>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct CommitRequest {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CommitResult {
    pub imported: usize,
    pub skipped: usize,
}

/// Ranks for `n` new books in order, all before the first inactive book.
async fn ranks_for(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    n: usize,
) -> sqlx::Result<Vec<String>> {
    let first: Option<String> = sqlx::query_scalar(
        "SELECT MIN(b.rank) FROM books b WHERE b.user_id = ? AND b.deleted = 0 AND b.active = 0
           AND NOT EXISTS (SELECT 1 FROM readings r WHERE r.book_id = b.id AND r.deleted = 0 AND r.finished_on IS NOT NULL
                           AND r.id = (SELECT MAX(id) FROM readings x WHERE x.book_id = b.id AND x.deleted = 0))",
    )
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    let mut out = Vec::with_capacity(n);
    let mut prev: Option<String> = None;
    for _ in 0..n {
        let r = mau_core::rank::between(prev.as_deref(), first.as_deref());
        prev = Some(r.clone());
        out.push(r);
    }
    Ok(out)
}

const BOOK_FIELDS: [&str; 7] = [
    "rank",
    "active",
    "rating",
    "notes",
    "title_override",
    "authors_override",
    "cover_override",
];

/// Rows the commit will create.
fn to_create(results: &[Matched], choices: &HashMap<usize, &Choice>) -> usize {
    results
        .iter()
        .enumerate()
        .filter(|(i, m)| {
            m.kind != MatchKind::Dupe && choices.get(i).is_none_or(|c| c.action != "skip")
        })
        .count()
}

/// Starts saving in the background; `GET /api/import/{job}` reports
/// progress and the result. Saving many books takes minutes (each matched
/// book's editions and cover are fetched), longer than a proxy waits for one
/// request.
pub async fn commit(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> ApiResult<Json<SaveProgress>> {
    let job = job_of(&state, &id, &s.user.id)?;
    let total = {
        let mut j = job.lock().unwrap();
        if !j.matched() {
            return Err(ApiError::conflict(
                "not_ready",
                "Still matching books. Try again in a moment.",
            ));
        }
        if j.saving.is_some() {
            return Err(ApiError::conflict(
                "saving",
                "This import has already been saved.",
            ));
        }
        let results: Vec<Matched> = j.results.iter().flatten().cloned().collect();
        let choices: HashMap<usize, &Choice> = req.choices.iter().map(|c| (c.index, c)).collect();
        let total = to_create(&results, &choices);
        j.saving = Some(Saving::Running { done: 0, total });
        total
    };
    let user_id = s.user.id.clone();
    tokio::spawn(async move {
        let outcome = match save(&state, &user_id, &job, req).await {
            Ok(r) => Saving::Done(r),
            Err(e) => {
                tracing::error!("import: saving failed: {}", e.message);
                Saving::Failed(e.message)
            }
        };
        job.lock().unwrap().saving = Some(outcome);
    });
    Ok(Json(SaveProgress { done: 0, total }))
}

async fn save(
    state: &AppState,
    user_id: &str,
    job: &Arc<Mutex<Job>>,
    req: CommitRequest,
) -> ApiResult<CommitResult> {
    let (rows, mau, results) = {
        let j = job.lock().unwrap();
        (
            j.rows.clone(),
            j.mau.clone(),
            j.results.iter().flatten().cloned().collect::<Vec<_>>(),
        )
    };
    let choices: HashMap<usize, &Choice> = req.choices.iter().map(|c| (c.index, c)).collect();

    // Phase 1: catalog works for matched rows (each in its own transaction).
    enum Plan {
        Work(String),
        Manual,
    }
    let mut plans: Vec<(usize, Plan)> = Vec::new();
    let mut skipped = 0;
    let mut done = 0;
    for (i, m) in results.iter().enumerate() {
        let default = match m.kind {
            MatchKind::Matched | MatchKind::Uncertain => "match",
            MatchKind::Unmatched => "manual",
            MatchKind::Dupe => "skip",
        };
        let action = choices.get(&i).map_or(default, |c| c.action.as_str());
        if m.kind == MatchKind::Dupe || action == "skip" {
            skipped += 1;
            continue;
        }
        if let Some(Saving::Running { done: d, .. }) = &mut job.lock().unwrap().saving {
            *d = done;
        }
        done += 1;
        if action == "manual" {
            plans.push((i, Plan::Manual));
            continue;
        }
        if let Some(w) = &m.local_work {
            plans.push((i, Plan::Work(w.clone())));
            continue;
        }
        let k = choices.get(&i).and_then(|c| c.candidate).unwrap_or(0);
        match m.candidates.get(k) {
            Some(c) => match catalog::ensure_candidate(state, c, rows[i].isbn13.as_deref()).await {
                Ok(w) => plans.push((i, Plan::Work(w.id))),
                Err(e) => {
                    tracing::warn!(
                        "import: couldn't add {:?} to the catalog: {}",
                        rows[i].title,
                        e.message
                    );
                    plans.push((i, Plan::Manual));
                }
            },
            None => plans.push((i, Plan::Manual)),
        }
    }

    // Phase 2: the library rows, in one transaction.
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let ranks = ranks_for(&mut tx, user_id, plans.len()).await?;
    let mut clock = state.clock.lock().unwrap().now(now_ms() as u64);
    let mut next_hlc = || {
        let h = clock.to_string();
        clock.counter = clock.counter.saturating_add(1);
        h
    };
    for ((i, plan), rank) in plans.iter().zip(ranks) {
        let row = &rows[*i];
        let full = mau.as_ref().map(|m| &m[*i]);
        let work_id = match plan {
            Plan::Work(w) => w.clone(),
            Plan::Manual => {
                let id = new_id();
                let people: Vec<Person> = match full {
                    Some(b) => b.people.clone(),
                    None => row
                        .authors
                        .iter()
                        .map(|a| Person {
                            name: a.clone(),
                            role: "author".into(),
                        })
                        .collect(),
                };
                let people = library::clean_people(&people).unwrap_or_default();
                sqlx::query("INSERT INTO works (id, title, first_year, owner_id, source, created_at, seq) VALUES (?, ?, ?, ?, 'import', ?, ?)")
                    .bind(&id)
                    .bind(row.title.chars().take(300).collect::<String>())
                    .bind(row.year)
                    .bind(user_id)
                    .bind(now_ms())
                    .bind(seq)
                    .execute(&mut *tx)
                    .await?;
                library::insert_people(&mut tx, &id, &people).await?;
                let editions: Vec<library::NewEdition> = match full {
                    Some(b) => b
                        .editions
                        .iter()
                        .map(|e| library::NewEdition {
                            id: new_id(),
                            publisher: e.publisher.clone(),
                            binding: e.binding.clone(),
                            year: e.year,
                            isbn13: e.isbn13.clone(),
                            pages: e.pages,
                        })
                        .collect(),
                    None if row.isbn13.is_some() || row.publisher.is_some() => {
                        vec![library::NewEdition {
                            id: new_id(),
                            publisher: row.publisher.clone(),
                            binding: row.binding.clone(),
                            year: None,
                            isbn13: row.isbn13.clone(),
                            pages: row.pages,
                        }]
                    }
                    None => vec![],
                };
                for e in &editions {
                    library::insert_edition(&mut tx, &id, Some(user_id), e, seq).await?;
                }
                id
            }
        };

        let h = next_hlc();
        let book_id = new_id();
        let clock_json = serde_json::to_string(
            &BOOK_FIELDS
                .iter()
                .map(|f| (*f, h.as_str()))
                .collect::<HashMap<_, _>>(),
        )
        .unwrap();
        let active = full.is_some_and(|b| {
            b.active && b.readings.last().is_some_and(|r| r.finished_on.is_none())
        });
        sqlx::query(
            "INSERT INTO books (id, user_id, work_id, rank, active, rating, notes, title_override, authors_override, added_at, clock, seq)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&book_id)
        .bind(user_id)
        .bind(&work_id)
        .bind(&rank)
        .bind(active)
        .bind(row.rating.clamp(0, 5))
        .bind(row.notes.chars().take(20_000).collect::<String>())
        .bind(full.and_then(|b| b.title_override.clone()))
        .bind(full.and_then(|b| b.authors_override.as_ref()).map(|a| serde_json::to_string(a).unwrap()))
        .bind(full.map(|b| b.added_at).or(row.added_at).unwrap_or_else(now_ms))
        .bind(&clock_json)
        .bind(seq)
        .execute(&mut *tx)
        .await?;

        let mut tags: Vec<String> = row
            .tags
            .iter()
            .map(|t| t.trim().chars().take(60).collect())
            .filter(|t: &String| !t.is_empty())
            .collect();
        tags.sort();
        tags.dedup();
        for t in tags {
            sqlx::query("INSERT INTO book_tags (book_id, user_id, tag, present, clock, seq) VALUES (?, ?, ?, 1, ?, ?)")
                .bind(&book_id)
                .bind(user_id)
                .bind(t)
                .bind(&h)
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }

        // Copies.
        let copies: Vec<(String, String, Option<String>)> = match full {
            Some(b) => b
                .copies
                .iter()
                .map(|c| {
                    (
                        c.format.clone(),
                        c.location.clone(),
                        c.edition_isbn13.clone(),
                    )
                })
                .collect(),
            None if row.owned => vec![(
                row.binding.clone().unwrap_or_else(|| "Paperback".into()),
                String::new(),
                row.isbn13.clone(),
            )],
            None => vec![],
        };
        for (format, location, isbn) in copies {
            let format = if library::FORMATS.contains(&format.as_str()) {
                format
            } else {
                "Paperback".into()
            };
            let edition: Option<String> = match &isbn {
                Some(i) => {
                    sqlx::query_scalar(
                        "SELECT id FROM editions WHERE work_id = ? AND isbn13 = ? LIMIT 1",
                    )
                    .bind(&work_id)
                    .bind(i)
                    .fetch_optional(&mut *tx)
                    .await?
                }
                None => None,
            };
            sqlx::query("INSERT INTO copies (id, book_id, user_id, format, location, edition_id, clock, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(new_id())
                .bind(&book_id)
                .bind(user_id)
                .bind(format)
                .bind(location)
                .bind(edition)
                .bind(format!(r#"{{"format":"{h}","location":"{h}","edition_id":"{h}"}}"#))
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }

        // Readings: finished reads from other services (no daily history), or
        // full readings with their events from a mau export.
        let catalog_pages: Option<i64> =
            sqlx::query_scalar("SELECT MAX(pages) FROM editions WHERE work_id = ?")
                .bind(&work_id)
                .fetch_one(&mut *tx)
                .await?;
        let readings: Vec<export::Reading> = match full {
            Some(b) => b.readings.clone(),
            None => row
                .reads
                .iter()
                .map(|r| {
                    let total = row.pages.or(catalog_pages).unwrap_or(1).max(1);
                    let finished = r.finished.clone().unwrap_or_else(|| UNKNOWN_DATE.into());
                    export::Reading {
                        started_on: r.started.clone().unwrap_or_else(|| finished.clone()),
                        finished_on: Some(finished),
                        total,
                        pos: total,
                        events: vec![],
                    }
                })
                .collect(),
        };
        for r in readings {
            let reading_id = new_id();
            let rh = next_hlc();
            sqlx::query(
                "INSERT INTO readings (id, book_id, user_id, started_on, finished_on, total, pos, pos_clock, clock, seq)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&reading_id)
            .bind(&book_id)
            .bind(user_id)
            .bind(&r.started_on)
            .bind(&r.finished_on)
            .bind(r.total.clamp(1, 100_000))
            .bind(r.pos.clamp(0, 100_000))
            .bind(&rh)
            .bind(format!(r#"{{"total":"{rh}","finished_on":"{rh}"}}"#))
            .bind(seq)
            .execute(&mut *tx)
            .await?;
            let mut last = rh;
            for e in &r.events {
                last = next_hlc();
                sqlx::query(
                    "INSERT INTO progress_events (id, reading_id, user_id, hlc, local_date, from_page, to_page, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(new_id())
                .bind(&reading_id)
                .bind(user_id)
                .bind(&last)
                .bind(&e.date)
                .bind(e.from)
                .bind(e.to)
                .bind(seq)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query("UPDATE readings SET pos_clock = ? WHERE id = ?")
                .bind(&last)
                .bind(&reading_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    let imported = plans.len();
    tx.commit().await?;
    {
        let mut c = state.clock.lock().unwrap();
        c.observe(&clock);
    }
    Ok(CommitResult { imported, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_tests::TestApp;
    use serde_json::{Value, json};

    /// Starts an import and waits for matching; returns the job and its rows.
    async fn start_import(
        app: &TestApp,
        cookie: &str,
        source: &str,
        text: &str,
    ) -> (String, Value) {
        let (status, started) = app
            .post(
                "/api/import",
                json!({ "source": source, "text": text }),
                Some(cookie),
            )
            .await;
        assert_eq!(status, 200, "{started}");
        let job = started["job"].as_str().unwrap().to_string();
        let mut rows = Value::Null;
        for _ in 0..100 {
            let (_, st) = app.get(&format!("/api/import/{job}"), Some(cookie)).await;
            if !st["rows"].is_null() {
                rows = st["rows"].clone();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(rows.is_array(), "matching never finished");
        (job, rows)
    }

    /// Saves an import and waits for the result.
    async fn save_import(app: &TestApp, cookie: &str, job: &str, choices: Value) -> Value {
        let (status, started) = app
            .post(
                &format!("/api/import/{job}/commit"),
                json!({ "choices": choices }),
                Some(cookie),
            )
            .await;
        assert_eq!(status, 200, "{started}");
        for _ in 0..200 {
            let (_, st) = app.get(&format!("/api/import/{job}"), Some(cookie)).await;
            assert!(st["error"].is_null(), "{st}");
            if !st["result"].is_null() {
                return st["result"].clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("saving never finished");
    }

    async fn run_import(app: &TestApp, cookie: &str, source: &str, text: &str) -> Value {
        let (job, rows) = start_import(app, cookie, source, text).await;
        let done = save_import(app, cookie, &job, json!([])).await;
        json!({ "rows": rows, "done": done })
    }

    async fn offline(app: &TestApp) {
        let none: Vec<crate::providers::ProviderConfig> = vec![];
        crate::settings::set(&app.state.db, "providers", &none)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn correcting_a_row_then_saving_in_the_background() {
        let app = TestApp::new().await;
        offline(&app).await;
        let (_, cookie) = app.user("rri", false).await;
        let json = r#"{"contents": {"works": {
            "a": {"data": "Dun Mesiah | Frank Herbret", "created": "2022-06-05T03:24:57.708Z", "updated": "2022-06-05T03:24:57.708Z"},
            "b": {"data": "Cosmos | Carl Sagan", "created": "2022-06-05T02:54:14.625Z", "updated": "2022-06-05T03:19:27.548Z"}
        }}}"#;
        let (job, rows) = start_import(&app, &cookie, "dinky", json).await;
        assert_eq!(rows[0]["kind"], "unmatched");
        let (_, cur) = app.get("/api/import/current", Some(&cookie)).await;
        assert_eq!(cur["job"], job.as_str());

        // Correct the first row; with no providers it's still not found, but
        // the manual entry uses the correction.
        let (status, row) = app
            .post(
                &format!("/api/import/{job}/rows/0"),
                json!({ "title": " Dune Messiah ", "authors": ["Frank Herbert", " "] }),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 200, "{row}");
        assert_eq!(row["title"], "Dune Messiah");
        assert_eq!(row["authors"], json!(["Frank Herbert"]));
        let (status, _) = app
            .post(
                &format!("/api/import/{job}/rows/0"),
                json!({ "title": "", "authors": [] }),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 400);

        let done = save_import(
            &app,
            &cookie,
            &job,
            json!([{ "index": 1, "action": "skip" }]),
        )
        .await;
        assert_eq!(done, json!({ "imported": 1, "skipped": 1 }));
        // Saved once only.
        let (status, _) = app
            .post(
                &format!("/api/import/{job}/commit"),
                json!({ "choices": [] }),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 409);
        let (_, sync) = app
            .post("/api/sync", json!({ "since": 0, "ops": [] }), Some(&cookie))
            .await;
        assert_eq!(sync["changes"]["works"][0]["title"], "Dune Messiah");
        let people = &sync["changes"]["works"][0]["people"];
        assert_eq!(people[0]["name"], "Frank Herbert", "{people}");

        // Dismissed, it's gone.
        let (status, _, _) = app
            .request("DELETE", &format!("/api/import/{job}"), None, Some(&cookie))
            .await;
        assert_eq!(status, 200);
        let (_, cur) = app.get("/api/import/current", Some(&cookie)).await;
        assert!(cur["job"].is_null());
    }

    #[tokio::test]
    async fn own_google_books_key() {
        let app = TestApp::new().await;
        let (_, cookie) = app.user("rri", false).await;
        let (_, k) = app.get("/api/account/google-books", Some(&cookie)).await;
        assert!(k["ends_with"].is_null());
        let (status, _, _) = app
            .request(
                "PUT",
                "/api/account/google-books",
                Some(json!({ "key": "not a key" })),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 400);
        let (status, k, _) = app
            .request(
                "PUT",
                "/api/account/google-books",
                Some(json!({ "key": " AIzaSyExample1a2b " })),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 200, "{k}");
        assert_eq!(k["ends_with"], "1a2b");
        let (_, st) = app.get("/api/auth/status", Some(&cookie)).await;
        assert_eq!(st["me"]["google_books"], true, "{st}");
        let stored: Option<String> =
            sqlx::query_scalar("SELECT google_books_key FROM users WHERE username = 'rri'")
                .fetch_one(&app.state.db)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some("AIzaSyExample1a2b"));
        let (status, k, _) = app
            .request("DELETE", "/api/account/google-books", None, Some(&cookie))
            .await;
        assert_eq!(status, 200);
        assert!(k["ends_with"].is_null());
    }

    #[test]
    fn fallback_searches() {
        assert_eq!(
            fallback_query(
                "Designing Data-Intensive Applications: The Big Ideas",
                Some("Martin Kleppmann")
            ),
            "Designing Data Intensive Applications Kleppmann"
        );
        assert_eq!(
            fallback_query("Don't Make Me Think (Revised)", Some("Steve Krug")),
            "Dont Make Me Think Krug"
        );
        assert_eq!(
            fallback_query("Taxes Made Simple", Some("Mike Piper, CPA")),
            "Taxes Made Simple Piper"
        );
        assert_eq!(fallback_query("Cosmos", None), "Cosmos");
        assert_eq!(norm_author("Bird, Richard"), "bird");
        assert_eq!(norm_author("Ernst W Mayr"), "mayr");
    }

    #[test]
    fn clear_matches() {
        let c = |title: &str, authors: &[&str]| Candidate {
            provider: "openlibrary".into(),
            source_id: "x".into(),
            title: title.into(),
            authors: authors.iter().map(|a| a.to_string()).collect(),
            first_year: None,
            cover_url: None,
            edition_count: None,
            editions: vec![],
        };
        let row = Row {
            title: "Dune Messiah (Dune Series, #2)".into(),
            authors: vec!["Frank Herbert".into()],
            ..Default::default()
        };
        assert!(is_match(&c("Dune Messiah", &["Frank Herbert"]), &row));
        assert!(is_match(
            &c("Dune messiah: a novel", &["Herbert, Frank"]),
            &row
        ));
        assert!(!is_match(&c("Dune Messiah", &["Brian Herbert Jr"]), &row));
        assert!(!is_match(&c("Dune", &["Frank Herbert"]), &row));
    }

    #[tokio::test]
    async fn goodreads_import_then_mau_round_trip() {
        let app = TestApp::new().await;
        // No providers: every row becomes a manual (private) entry, offline.
        let none: Vec<crate::providers::ProviderConfig> = vec![];
        crate::settings::set(&app.state.db, "providers", &none)
            .await
            .unwrap();
        let (_, cookie) = app.user("rri", false).await;
        let csv = "Book Id,Title,Author,Additional Authors,ISBN,ISBN13,My Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Exclusive Shelf,My Review,Private Notes,Read Count,Owned Copies
1,Piranesi,Susanna Clarke,,,\"=\"\"9781635575637\"\"\",4,Bloomsbury,Hardcover,272,2020,2020,2026/10/20,2026/09/28,\"favorites, read\",read,Lovely.,,1,1
2,Middlemarch,George Eliot,,,,0,,Paperback,880,1994,1871,,2026/08/01,currently-reading,currently-reading,,,0,0
";
        let res = run_import(&app, &cookie, "goodreads", csv).await;
        assert_eq!(res["done"]["imported"], 2);
        assert_eq!(res["rows"][0]["kind"], "unmatched");

        let (_, sync) = app
            .post("/api/sync", json!({ "since": 0, "ops": [] }), Some(&cookie))
            .await;
        let ch = &sync["changes"];
        assert_eq!(ch["books"].as_array().unwrap().len(), 2);
        let piranesi = ch["works"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["title"] == "Piranesi")
            .unwrap();
        assert_eq!(piranesi["private"], true);
        assert_eq!(ch["readings"].as_array().unwrap().len(), 1);
        assert_eq!(ch["readings"][0]["finished_on"], "2026-10-20");
        assert_eq!(ch["readings"][0]["total"], 272);
        assert_eq!(ch["copies"][0]["format"], "Hardcover");
        assert_eq!(ch["tags"][0]["tag"], "favorites");
        assert!(
            ch["books"]
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["active"] == false)
        );

        // Importing the same file again: both are already on the list.
        let again = run_import(&app, &cookie, "goodreads", csv).await;
        assert_eq!(again["done"]["skipped"], 2);

        // Export as JSON and import it into a fresh account.
        let res = router_get_text(&app, "/api/export.json", &cookie).await;
        let (_, other) = app.user("juno", false).await;
        let res = run_import(&app, &other, "mau", &res).await;
        assert_eq!(res["done"]["imported"], 2);
        let (_, sync) = app
            .post("/api/sync", json!({ "since": 0, "ops": [] }), Some(&other))
            .await;
        assert_eq!(sync["changes"]["readings"][0]["finished_on"], "2026-10-20");
        assert_eq!(
            sync["changes"]["books"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["rating"] == 4)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn dinky_import() {
        let app = TestApp::new().await;
        let none: Vec<crate::providers::ProviderConfig> = vec![];
        crate::settings::set(&app.state.db, "providers", &none)
            .await
            .unwrap();
        let (_, cookie) = app.user("rri", false).await;
        let json = r#"{"contents": {"works": {
            "a": {"data": "Dune Messiah (Dune Series, #2) | Frank Herbert", "created": "2022-06-05T03:24:57.708Z", "updated": "2022-06-05T03:24:57.708Z"},
            "b": {"data": "Cosmos | Carl Sagan", "created": "2022-06-05T02:54:14.625Z", "updated": "2022-06-05T03:19:27.548Z", "archive": true},
            "c": {"data": "Gone | Nobody", "created": "2022-06-05T02:54:14.625Z", "updated": "2026-11-01T13:20:08.034Z", "deleted": "2026-11-01T13:20:08.034Z"}
        }}}"#;
        let res = run_import(&app, &cookie, "dinky", json).await;
        assert_eq!(res["done"]["imported"], 2);
        let (_, sync) = app
            .post("/api/sync", json!({ "since": 0, "ops": [] }), Some(&cookie))
            .await;
        let ch = &sync["changes"];
        let works = ch["works"].as_array().unwrap();
        // Manual entries keep the title as written.
        assert!(
            works
                .iter()
                .any(|w| w["title"] == "Dune Messiah (Dune Series, #2)")
        );
        let readings = ch["readings"].as_array().unwrap();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0]["finished_on"], UNKNOWN_DATE);
        let books = ch["books"].as_array().unwrap();
        assert!(books.iter().all(|b| b["active"] == false));
        assert!(books.iter().any(|b| b["added_at"] == 1_654_399_497_708i64));

        // Again: both already on the list, with or without the note.
        let again = run_import(&app, &cookie, "dinky", json).await;
        assert_eq!(again["done"]["skipped"], 2);
        let (_, other) = app.user("juno", false).await;
        let csv = "Book Id,Title,Author,Additional Authors,ISBN,ISBN13,My Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Exclusive Shelf,My Review,Private Notes,Read Count,Owned Copies
1,Dune Messiah,Frank Herbert,,,,0,,,,,,,,to-read,to-read,,,0,0
";
        run_import(&app, &other, "goodreads", csv).await;
        let res = run_import(&app, &other, "dinky", json).await;
        assert_eq!(res["rows"][0]["kind"], "dupe");
        assert_eq!(res["done"]["imported"], 1);
    }

    async fn router_get_text(app: &TestApp, path: &str, cookie: &str) -> String {
        use axum::body::Body;
        use axum::http::{Request, header};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let req = Request::get(path)
            .header(header::COOKIE, format!("mau_session={cookie}"))
            .body(Body::empty())
            .unwrap();
        let res = crate::app::router(app.state.clone())
            .oneshot(req)
            .await
            .unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[test]
    fn title_normalization() {
        assert_eq!(
            norm_title("The Left Hand of Darkness: A Novel"),
            "lefthandofdarkness"
        );
        assert_eq!(norm_title("Dune"), "dune");
        assert_eq!(norm_author("Ursula K. Le Guin"), "guin");
    }
}
