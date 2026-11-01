//! Importing from other services, with review before anything changes.
//!
//! 1. `POST /api/import` parses the file and starts matching each row to the
//!    catalog in the background.
//! 2. `GET /api/import/{job}` reports progress, then each row's match:
//!    matched, uncertain (choose a candidate), unmatched (add as a manual
//!    entry or skip), or already on the list (skipped).
//! 3. `POST /api/import/{job}/commit` creates the books.

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
use crate::providers::Candidate;
use crate::util::{HOUR_MS, new_id, now_ms};

/// Stands for "finished, date unknown" (spec: undated reads are finished).
pub const UNKNOWN_DATE: &str = "0000-01-01";
const MAX_ROWS: usize = 5000;
const CONCURRENCY: usize = 3;

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
}

struct Job {
    user_id: String,
    rows: Vec<Row>,
    /// Full history for mau's own exports.
    mau: Option<Vec<export::Book>>,
    results: Vec<Option<Matched>>,
    created_at: i64,
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
        map.retain(|_, j| j.lock().unwrap().created_at > cutoff);
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
    /// goodreads, storygraph, librarything, csv, or mau
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
        created_at: now_ms(),
    });
    let handle = state.imports.get(&job, &s.user.id).expect("just inserted");
    tokio::spawn(match_all(state.clone(), s.user.id.clone(), handle));
    Ok(Json(ImportStarted { job, total }))
}

// ---------------------------------------------------------------------------
// Matching

fn norm_title(t: &str) -> String {
    let t = t.split(':').next().unwrap_or(t).to_lowercase();
    let t = ["the ", "a ", "an "]
        .iter()
        .find_map(|a| t.strip_prefix(a))
        .unwrap_or(&t)
        .to_string();
    t.chars().filter(|c| c.is_alphanumeric()).collect()
}

fn norm_author(a: &str) -> String {
    a.split_whitespace()
        .last()
        .unwrap_or("")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
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

async fn match_row(state: &AppState, ex: &Existing, row: &Row, mau: bool) -> Matched {
    let dupe = row.isbn13.as_ref().is_some_and(|i| ex.isbns.contains(i))
        || ex.titles.contains(&(
            norm_title(&row.title),
            norm_author(row.authors.first().map_or("", |s| s)),
        ));
    if dupe {
        return Matched {
            kind: MatchKind::Dupe,
            candidates: vec![],
            local_work: None,
        };
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
                kind: MatchKind::Matched,
                candidates: vec![],
                local_work: local,
            };
        }
    }
    if mau {
        // A mau export carries everything needed; don't ask providers.
        return Matched {
            kind: MatchKind::Unmatched,
            candidates: vec![],
            local_work: None,
        };
    }
    if let Some(isbn) = &row.isbn13
        && let Ok((found, _, _)) = catalog::find_candidates(state, isbn, Some(isbn)).await
        && !found.is_empty()
    {
        return Matched {
            kind: MatchKind::Matched,
            candidates: found.into_iter().take(1).collect(),
            local_work: None,
        };
    }
    let query = format!("{} {}", row.title, row.authors.first().map_or("", |s| s));
    let found = catalog::find_candidates(state, query.trim(), None)
        .await
        .map(|(f, _, _)| f)
        .unwrap_or_default();
    if found.is_empty() {
        return Matched {
            kind: MatchKind::Unmatched,
            candidates: vec![],
            local_work: None,
        };
    }
    let want = (
        norm_title(&row.title),
        row.authors.first().map(|a| norm_author(a)),
    );
    let exact = found.iter().position(|c| {
        norm_title(&c.title) == want.0
            && want
                .1
                .as_ref()
                .is_none_or(|a| c.authors.iter().any(|x| &norm_author(x) == a))
    });
    match exact {
        Some(i) => Matched {
            kind: MatchKind::Matched,
            candidates: vec![found[i].clone()],
            local_work: None,
        },
        None => Matched {
            kind: MatchKind::Uncertain,
            candidates: found.into_iter().take(3).collect(),
            local_work: None,
        },
    }
}

async fn match_all(state: AppState, user_id: String, job: Arc<Mutex<Job>>) {
    let (rows, mau) = {
        let j = job.lock().unwrap();
        (j.rows.clone(), j.mau.is_some())
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
    futures::stream::iter(rows.into_iter().enumerate())
        .map(|(i, row)| {
            let (state, ex) = (state.clone(), ex.clone());
            async move { (i, match_row(&state, &ex, &row, mau).await) }
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
pub struct JobStatus {
    pub total: usize,
    pub done: usize,
    /// Present once every row is matched.
    pub rows: Option<Vec<RowStatus>>,
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

pub async fn status(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<JobStatus>> {
    let job = state
        .imports
        .get(&id, &s.user.id)
        .ok_or_else(|| ApiError::not_found("That import has expired. Start again."))?;
    let j = job.lock().unwrap();
    let done = j.results.iter().filter(|r| r.is_some()).count();
    let rows = (done == j.rows.len()).then(|| {
        j.rows
            .iter()
            .zip(&j.results)
            .enumerate()
            .map(|(index, (row, m))| {
                let m = m.as_ref().expect("all matched");
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
            })
            .collect()
    });
    Ok(Json(JobStatus {
        total: j.rows.len(),
        done,
        rows,
    }))
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

#[derive(Serialize, TS)]
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

pub async fn commit(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> ApiResult<Json<CommitResult>> {
    let job = state
        .imports
        .get(&id, &s.user.id)
        .ok_or_else(|| ApiError::not_found("That import has expired. Start again."))?;
    let (rows, mau, results) = {
        let j = job.lock().unwrap();
        if j.results.iter().any(Option::is_none) {
            return Err(ApiError::conflict(
                "not_ready",
                "Still matching books. Try again in a moment.",
            ));
        }
        (
            j.rows.clone(),
            j.mau.clone(),
            j.results
                .iter()
                .map(|r| r.clone().unwrap())
                .collect::<Vec<_>>(),
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
            Some(c) => {
                match catalog::ensure_candidate(&state, c, rows[i].isbn13.as_deref()).await {
                    Ok(w) => plans.push((i, Plan::Work(w.id))),
                    Err(e) => {
                        tracing::warn!(
                            "import: couldn't add {:?} to the catalog: {}",
                            rows[i].title,
                            e.message
                        );
                        plans.push((i, Plan::Manual));
                    }
                }
            }
            None => plans.push((i, Plan::Manual)),
        }
    }

    // Phase 2: the library rows, in one transaction.
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let ranks = ranks_for(&mut tx, &s.user.id, plans.len()).await?;
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
                    .bind(&s.user.id)
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
                    library::insert_edition(&mut tx, &id, Some(&s.user.id), e, seq).await?;
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
        .bind(&s.user.id)
        .bind(&work_id)
        .bind(&rank)
        .bind(active)
        .bind(row.rating.clamp(0, 5))
        .bind(row.notes.chars().take(20_000).collect::<String>())
        .bind(full.and_then(|b| b.title_override.clone()))
        .bind(full.and_then(|b| b.authors_override.as_ref()).map(|a| serde_json::to_string(a).unwrap()))
        .bind(full.map_or_else(now_ms, |b| b.added_at))
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
                .bind(&s.user.id)
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
                .bind(&s.user.id)
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
            .bind(&s.user.id)
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
                .bind(&s.user.id)
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
    state.imports.remove(&id);
    Ok(Json(CommitResult { imported, skipped }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_tests::TestApp;
    use serde_json::{Value, json};

    async fn run_import(app: &TestApp, cookie: &str, source: &str, text: &str) -> Value {
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
        let (status, done) = app
            .post(
                &format!("/api/import/{job}/commit"),
                json!({ "choices": [] }),
                Some(cookie),
            )
            .await;
        assert_eq!(status, 200, "{done}");
        json!({ "rows": rows, "done": done })
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
