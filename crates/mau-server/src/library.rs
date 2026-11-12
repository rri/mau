//! A user's library and its sync protocol.
//!
//! Devices send operations (`Op`) and pull changed rows. Each operation is
//! applied with field-level last-writer-wins: a field changes only if the
//! operation's HLC is newer than the field's clock. Progress uses a
//! fork-aware rule so that the later of two concurrent offline sessions wins
//! and the other's page events stop counting. See docs/DECISIONS.md.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::Json;
use axum::extract::State;
use mau_core::hlc::Hlc;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, Transaction};
use ts_rs::TS;

use crate::app::AppState;
use crate::auth::session::Authed;
use crate::error::{ApiError, ApiResult};

pub type Clock = BTreeMap<String, String>;

// ---------------------------------------------------------------------------
// Rows sent to devices

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Person {
    pub name: String,
    /// author, translator, editor, or illustrator
    pub role: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EditionRow {
    pub id: String,
    pub work_id: String,
    pub publisher: Option<String>,
    pub binding: Option<String>,
    #[ts(type = "number | null")]
    pub year: Option<i64>,
    pub isbn13: Option<String>,
    #[ts(type = "number | null")]
    pub pages: Option<i64>,
    pub cover: Option<String>,
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WorkRow {
    pub id: String,
    pub title: String,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
    pub cover: Option<String>,
    /// A manual entry visible only to its creator.
    pub private: bool,
    pub suggested: bool,
    pub people: Vec<Person>,
    pub editions: Vec<EditionRow>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BookRow {
    pub id: String,
    pub work_id: String,
    pub rank: String,
    pub active: bool,
    #[ts(type = "number")]
    pub rating: i64,
    pub notes: String,
    pub title_override: Option<String>,
    pub authors_override: Option<Vec<String>>,
    pub cover_override: Option<String>,
    #[ts(type = "number")]
    pub added_at: i64,
    pub deleted: bool,
    pub clock: Clock,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TagRow {
    pub book_id: String,
    pub tag: String,
    pub present: bool,
    pub clock: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CopyRow {
    pub id: String,
    pub book_id: String,
    pub format: String,
    pub location: String,
    pub edition_id: Option<String>,
    pub deleted: bool,
    pub clock: Clock,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReadingRow {
    pub id: String,
    pub book_id: String,
    pub started_on: String,
    pub finished_on: Option<String>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub pos: i64,
    pub pos_clock: String,
    pub deleted: bool,
    pub clock: Clock,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EventRow {
    pub id: String,
    pub reading_id: String,
    pub hlc: String,
    pub local_date: String,
    #[ts(type = "number")]
    pub from_page: i64,
    #[ts(type = "number")]
    pub to_page: i64,
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// Operations sent by devices

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct Op {
    pub id: String,
    pub hlc: String,
    pub op: OpKind,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct BookFields {
    #[ts(optional)]
    pub rank: Option<String>,
    #[ts(optional)]
    pub active: Option<bool>,
    #[ts(as = "Option<i32>", optional)]
    pub rating: Option<i64>,
    #[ts(optional)]
    pub notes: Option<String>,
    /// `""` clears the override.
    #[ts(optional)]
    pub title_override: Option<String>,
    /// An empty list clears the override.
    #[ts(optional)]
    pub authors_override: Option<Vec<String>>,
    /// `""` clears the override.
    #[ts(optional)]
    pub cover_override: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct CopyFields {
    #[ts(optional)]
    pub format: Option<String>,
    #[ts(optional)]
    pub location: Option<String>,
    /// `""` means unknown edition.
    #[ts(optional)]
    pub edition_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct ReadingFields {
    #[ts(as = "Option<i32>", optional)]
    pub total: Option<i64>,
    /// `""` means not finished.
    #[ts(optional)]
    pub finished_on: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct NewEdition {
    pub id: String,
    pub publisher: Option<String>,
    pub binding: Option<String>,
    #[ts(type = "number | null")]
    pub year: Option<i64>,
    pub isbn13: Option<String>,
    #[ts(type = "number | null")]
    pub pages: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewWork {
    pub id: String,
    pub title: String,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
    pub people: Vec<Person>,
    pub edition: Option<NewEdition>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum OpKind {
    #[serde(rename = "book.create")]
    BookCreate {
        id: String,
        work_id: String,
        rank: String,
        #[ts(type = "number")]
        added_at: i64,
    },
    #[serde(rename = "book.set")]
    BookSet { id: String, fields: BookFields },
    #[serde(rename = "book.delete")]
    BookDelete { id: String },
    #[serde(rename = "tag.set")]
    TagSet {
        book_id: String,
        tag: String,
        present: bool,
    },
    #[serde(rename = "copy.create")]
    CopyCreate {
        id: String,
        book_id: String,
        format: String,
        location: String,
        edition_id: Option<String>,
    },
    #[serde(rename = "copy.set")]
    CopySet { id: String, fields: CopyFields },
    #[serde(rename = "copy.delete")]
    CopyDelete { id: String },
    #[serde(rename = "reading.create")]
    ReadingCreate {
        id: String,
        book_id: String,
        started_on: String,
        #[ts(type = "number")]
        total: i64,
    },
    #[serde(rename = "reading.set")]
    ReadingSet { id: String, fields: ReadingFields },
    /// Removes a reading and its events (undoing "start reading").
    #[serde(rename = "reading.delete")]
    ReadingDelete { id: String },
    /// Moves the position. `base` is the position clock the device saw.
    #[serde(rename = "progress")]
    Progress {
        event_id: String,
        reading_id: String,
        base: String,
        #[ts(type = "number")]
        from: i64,
        #[ts(type = "number")]
        to: i64,
        local_date: String,
    },
    /// Takes back a progress event (the undo toast).
    #[serde(rename = "progress.undo")]
    ProgressUndo {
        reading_id: String,
        event_id: String,
        base: String,
        #[ts(type = "number")]
        to: i64,
    },
    /// A manual catalog entry, private to its creator.
    #[serde(rename = "work.create")]
    WorkCreate { work: NewWork },
}

// ---------------------------------------------------------------------------
// Validation

const MAX_PAGES: i64 = 100_000;

pub const FORMATS: &[&str] = &["Hardcover", "Paperback", "Ebook", "Audiobook"];

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError::bad_request("bad_op", msg)
}

fn check_date(d: &str) -> ApiResult<()> {
    let ok = d.len() == 10
        && d.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    if ok {
        Ok(())
    } else {
        Err(bad(format!("bad date {d:?}")))
    }
}

fn check_pages(p: i64) -> ApiResult<()> {
    if (0..=MAX_PAGES).contains(&p) {
        Ok(())
    } else {
        Err(bad("page out of range"))
    }
}

/// IDs of new records come from clients (UUIDs today); keep them short and plain.
fn check_id(id: &str) -> ApiResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(bad("bad id"));
    }
    Ok(())
}

/// The IDs an operation would create.
fn new_ids(op: &OpKind) -> Vec<&str> {
    use OpKind::*;
    match op {
        BookCreate { id, .. } | CopyCreate { id, .. } | ReadingCreate { id, .. } => vec![id],
        Progress { event_id, .. } => vec![event_id],
        WorkCreate { work } => std::iter::once(work.id.as_str())
            .chain(work.edition.as_ref().map(|e| e.id.as_str()))
            .collect(),
        _ => vec![],
    }
}

/// Stored covers are named by a 32-character hex hash (see `covers::store`).
pub fn is_cover_hash(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn check_hlc(h: &str) -> ApiResult<()> {
    h.parse::<Hlc>()
        .map(|_| ())
        .map_err(|_| bad(format!("bad clock {h:?}")))
}

fn clean_tag(t: &str) -> ApiResult<String> {
    let t = t.trim();
    if t.is_empty() || t.chars().count() > 60 {
        return Err(bad("Tags are 1–60 characters."));
    }
    Ok(t.to_string())
}

fn clean_text(s: &str, max: usize, what: &str) -> ApiResult<String> {
    let s = s.trim();
    if s.chars().count() > max {
        return Err(bad(format!("{what} is too long")));
    }
    Ok(s.to_string())
}

// ---------------------------------------------------------------------------
// Applying operations

/// Allocates the change number for this transaction. Called first so the
/// transaction takes SQLite's write lock before reading anything.
pub async fn next_seq(conn: &mut SqliteConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("UPDATE sync_seq SET n = n + 1 WHERE id = 1 RETURNING n")
        .fetch_one(conn)
        .await
}

async fn current_seq(conn: &mut SqliteConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT n FROM sync_seq WHERE id = 1")
        .fetch_one(conn)
        .await
}

fn parse_clock(s: &str) -> Clock {
    serde_json::from_str(s).unwrap_or_default()
}

/// True if `hlc` beats the field's clock, and records it.
fn claim(clock: &mut Clock, field: &str, hlc: &str) -> bool {
    match clock.get(field) {
        Some(c) if c.as_str() >= hlc => false,
        _ => {
            clock.insert(field.to_string(), hlc.to_string());
            true
        }
    }
}

fn full_clock(fields: &[&str], hlc: &str) -> String {
    let c: Clock = fields
        .iter()
        .map(|f| (f.to_string(), hlc.to_string()))
        .collect();
    serde_json::to_string(&c).unwrap()
}

/// What happened to an operation.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Yes,
    /// Ignored: stale, about a deleted or unknown row, or a duplicate.
    Dropped,
}

/// Whether a work is visible to the user (shared, or their own manual entry).
async fn work_visible(
    conn: &mut SqliteConnection,
    user_id: &str,
    work_id: &str,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM works WHERE id = ? AND (owner_id IS NULL OR owner_id = ?))",
    )
    .bind(work_id)
    .bind(user_id)
    .fetch_one(conn)
    .await
}

/// The book's owner and deleted flag, if it exists.
async fn book_owner(
    conn: &mut SqliteConnection,
    book_id: &str,
) -> sqlx::Result<Option<(String, bool)>> {
    sqlx::query_as("SELECT user_id, deleted FROM books WHERE id = ?")
        .bind(book_id)
        .fetch_optional(conn)
        .await
}

async fn live_book(
    conn: &mut SqliteConnection,
    user_id: &str,
    book_id: &str,
) -> sqlx::Result<bool> {
    Ok(matches!(book_owner(conn, book_id).await?, Some((u, false)) if u == user_id))
}

/// Progress chain context for one operation (see `mau_core::progress`).
pub struct ChainCtx<'a> {
    pub info: &'a mau_core::progress::ChainInfo,
    /// Outcome per chain in this batch: `None` until its first change is judged.
    pub outcomes: &'a mut Vec<Option<bool>>,
}

pub async fn apply(
    conn: &mut SqliteConnection,
    user_id: &str,
    seq: i64,
    op: &Op,
    chain: Option<ChainCtx<'_>>,
) -> ApiResult<Applied> {
    check_hlc(&op.hlc)?;
    for id in new_ids(&op.op) {
        check_id(id)?;
    }
    let hlc = op.hlc.as_str();
    use OpKind::*;
    match &op.op {
        BookCreate {
            id,
            work_id,
            rank,
            added_at,
        } => {
            if !mau_core::rank::is_valid(rank) {
                return Err(bad("bad rank"));
            }
            if !work_visible(conn, user_id, work_id).await? {
                return Ok(Applied::Dropped);
            }
            let fields = [
                "rank",
                "active",
                "rating",
                "notes",
                "title_override",
                "authors_override",
                "cover_override",
            ];
            let n = sqlx::query(
                "INSERT INTO books (id, user_id, work_id, rank, added_at, clock, seq) VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(id)
            .bind(user_id)
            .bind(work_id)
            .bind(rank)
            .bind(added_at)
            .bind(full_clock(&fields, hlc))
            .bind(seq)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            Ok(if n == 1 {
                Applied::Yes
            } else {
                Applied::Dropped
            })
        }
        BookSet { id, fields } => {
            let row: Option<(String, bool, String, bool, i64, String, Option<String>, Option<String>, Option<String>, String)> =
                sqlx::query_as(
                    "SELECT user_id, deleted, rank, active, rating, notes, title_override, authors_override, cover_override, clock
                     FROM books WHERE id = ?",
                )
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
            let Some((
                owner,
                deleted,
                mut rank,
                mut active,
                mut rating,
                mut notes,
                mut title_o,
                mut authors_o,
                mut cover_o,
                clock,
            )) = row
            else {
                return Ok(Applied::Dropped);
            };
            if owner != user_id || deleted {
                return Ok(Applied::Dropped);
            }
            let mut clock = parse_clock(&clock);
            let mut changed = false;
            if let Some(v) = &fields.rank {
                if !mau_core::rank::is_valid(v) {
                    return Err(bad("bad rank"));
                }
                if claim(&mut clock, "rank", hlc) {
                    rank = v.clone();
                    changed = true;
                }
            }
            if let Some(v) = fields.active
                && claim(&mut clock, "active", hlc)
            {
                active = v;
                changed = true;
            }
            if let Some(v) = fields.rating {
                if !(0..=5).contains(&v) {
                    return Err(bad("rating must be 0–5"));
                }
                if claim(&mut clock, "rating", hlc) {
                    rating = v;
                    changed = true;
                }
            }
            if let Some(v) = &fields.notes {
                let v = clean_text(v, 20_000, "notes")?;
                if claim(&mut clock, "notes", hlc) {
                    notes = v;
                    changed = true;
                }
            }
            if let Some(v) = &fields.title_override {
                let v = clean_text(v, 300, "title")?;
                if claim(&mut clock, "title_override", hlc) {
                    title_o = (!v.is_empty()).then_some(v);
                    changed = true;
                }
            }
            if let Some(v) = &fields.authors_override {
                let names: Vec<String> = v
                    .iter()
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect();
                if names.len() > 20 || names.iter().any(|n| n.chars().count() > 200) {
                    return Err(bad("too many or too long author names"));
                }
                if claim(&mut clock, "authors_override", hlc) {
                    authors_o = (!names.is_empty()).then(|| serde_json::to_string(&names).unwrap());
                    changed = true;
                }
            }
            if let Some(v) = &fields.cover_override
                && !v.is_empty()
                && !is_cover_hash(v)
            {
                return Err(bad("bad cover"));
            }
            if let Some(v) = &fields.cover_override
                && claim(&mut clock, "cover_override", hlc)
            {
                cover_o = (!v.is_empty()).then(|| v.clone());
                changed = true;
            }
            if !changed {
                return Ok(Applied::Dropped);
            }
            sqlx::query(
                "UPDATE books SET rank = ?, active = ?, rating = ?, notes = ?, title_override = ?, authors_override = ?,
                 cover_override = ?, clock = ?, seq = ? WHERE id = ?",
            )
            .bind(rank)
            .bind(active)
            .bind(rating)
            .bind(notes)
            .bind(title_o)
            .bind(authors_o)
            .bind(cover_o)
            .bind(serde_json::to_string(&clock).unwrap())
            .bind(seq)
            .bind(id)
            .execute(&mut *conn)
            .await?;
            Ok(Applied::Yes)
        }
        BookDelete { id } => {
            if !live_book(conn, user_id, id).await? {
                return Ok(Applied::Dropped);
            }
            // Removing a book from the list deletes all of its history.
            for sql in [
                "UPDATE readings SET deleted = 1, seq = ?1 WHERE book_id = ?2",
                "UPDATE copies SET deleted = 1, seq = ?1 WHERE book_id = ?2",
                "UPDATE book_tags SET present = 0, seq = ?1 WHERE book_id = ?2",
                "UPDATE progress_events SET deleted = 1, seq = ?1 WHERE reading_id IN (SELECT id FROM readings WHERE book_id = ?2)",
                "UPDATE books SET deleted = 1, notes = '', rating = 0, seq = ?1 WHERE id = ?2",
            ] {
                sqlx::query(sql)
                    .bind(seq)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
            }
            Ok(Applied::Yes)
        }
        TagSet {
            book_id,
            tag,
            present,
        } => {
            let tag = clean_tag(tag)?;
            if !live_book(conn, user_id, book_id).await? {
                return Ok(Applied::Dropped);
            }
            let n = sqlx::query(
                "INSERT INTO book_tags (book_id, user_id, tag, present, clock, seq) VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(book_id, tag) DO UPDATE SET present = excluded.present, clock = excluded.clock, seq = excluded.seq
                 WHERE excluded.clock > book_tags.clock",
            )
            .bind(book_id)
            .bind(user_id)
            .bind(&tag)
            .bind(present)
            .bind(hlc)
            .bind(seq)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            Ok(if n == 1 {
                Applied::Yes
            } else {
                Applied::Dropped
            })
        }
        CopyCreate {
            id,
            book_id,
            format,
            location,
            edition_id,
        } => {
            if !FORMATS.contains(&format.as_str()) {
                return Err(bad("unknown format"));
            }
            if !live_book(conn, user_id, book_id).await? {
                return Ok(Applied::Dropped);
            }
            let location = clean_text(location, 200, "location")?;
            let n = sqlx::query(
                "INSERT INTO copies (id, book_id, user_id, format, location, edition_id, clock, seq)
                 VALUES (?, ?, ?, ?, ?, (SELECT id FROM editions WHERE id = ?), ?, ?) ON CONFLICT(id) DO NOTHING",
            )
            .bind(id)
            .bind(book_id)
            .bind(user_id)
            .bind(format)
            .bind(location)
            .bind(edition_id.as_deref().unwrap_or(""))
            .bind(full_clock(&["format", "location", "edition_id"], hlc))
            .bind(seq)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            Ok(if n == 1 {
                Applied::Yes
            } else {
                Applied::Dropped
            })
        }
        CopySet { id, fields } => {
            let row: Option<(String, bool, String, String, Option<String>, String)> =
                sqlx::query_as("SELECT user_id, deleted, format, location, edition_id, clock FROM copies WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?;
            let Some((owner, deleted, mut format, mut location, mut edition, clock)) = row else {
                return Ok(Applied::Dropped);
            };
            if owner != user_id || deleted {
                return Ok(Applied::Dropped);
            }
            let mut clock = parse_clock(&clock);
            let mut changed = false;
            if let Some(v) = &fields.format {
                if !FORMATS.contains(&v.as_str()) {
                    return Err(bad("unknown format"));
                }
                if claim(&mut clock, "format", hlc) {
                    format = v.clone();
                    changed = true;
                }
            }
            if let Some(v) = &fields.location {
                let v = clean_text(v, 200, "location")?;
                if claim(&mut clock, "location", hlc) {
                    location = v;
                    changed = true;
                }
            }
            if let Some(v) = &fields.edition_id
                && claim(&mut clock, "edition_id", hlc)
            {
                edition = sqlx::query_scalar("SELECT id FROM editions WHERE id = ?")
                    .bind(v)
                    .fetch_optional(&mut *conn)
                    .await?;
                changed = true;
            }
            if !changed {
                return Ok(Applied::Dropped);
            }
            sqlx::query("UPDATE copies SET format = ?, location = ?, edition_id = ?, clock = ?, seq = ? WHERE id = ?")
                .bind(format)
                .bind(location)
                .bind(edition)
                .bind(serde_json::to_string(&clock).unwrap())
                .bind(seq)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            Ok(Applied::Yes)
        }
        CopyDelete { id } => {
            let n = sqlx::query("UPDATE copies SET deleted = 1, seq = ? WHERE id = ? AND user_id = ? AND deleted = 0")
                .bind(seq)
                .bind(id)
                .bind(user_id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
            Ok(if n == 1 {
                Applied::Yes
            } else {
                Applied::Dropped
            })
        }
        ReadingCreate {
            id,
            book_id,
            started_on,
            total,
        } => {
            check_date(started_on)?;
            if *total < 1 || *total > MAX_PAGES {
                return Err(bad("total pages out of range"));
            }
            if !live_book(conn, user_id, book_id).await? {
                return Ok(Applied::Dropped);
            }
            let n = sqlx::query(
                "INSERT INTO readings (id, book_id, user_id, started_on, total, pos_clock, clock, seq)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
            )
            .bind(id)
            .bind(book_id)
            .bind(user_id)
            .bind(started_on)
            .bind(total)
            .bind(hlc)
            .bind(full_clock(&["total", "finished_on"], hlc))
            .bind(seq)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            Ok(if n == 1 {
                Applied::Yes
            } else {
                Applied::Dropped
            })
        }
        ReadingSet { id, fields } => {
            let row: Option<(String, bool, i64, Option<String>, String)> = sqlx::query_as(
                "SELECT user_id, deleted, total, finished_on, clock FROM readings WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            let Some((owner, deleted, mut total, mut finished, clock)) = row else {
                return Ok(Applied::Dropped);
            };
            if owner != user_id || deleted {
                return Ok(Applied::Dropped);
            }
            let mut clock = parse_clock(&clock);
            let mut changed = false;
            if let Some(t) = fields.total {
                if !(1..=MAX_PAGES).contains(&t) {
                    return Err(bad("total pages out of range"));
                }
                if claim(&mut clock, "total", hlc) {
                    total = t;
                    changed = true;
                }
            }
            if let Some(f) = &fields.finished_on {
                if !f.is_empty() {
                    check_date(f)?;
                }
                if claim(&mut clock, "finished_on", hlc) {
                    finished = (!f.is_empty()).then(|| f.clone());
                    changed = true;
                }
            }
            if !changed {
                return Ok(Applied::Dropped);
            }
            sqlx::query(
                "UPDATE readings SET total = ?, finished_on = ?, clock = ?, seq = ? WHERE id = ?",
            )
            .bind(total)
            .bind(finished)
            .bind(serde_json::to_string(&clock).unwrap())
            .bind(seq)
            .bind(id)
            .execute(&mut *conn)
            .await?;
            Ok(Applied::Yes)
        }
        ReadingDelete { id } => {
            let n = sqlx::query("UPDATE readings SET deleted = 1, seq = ? WHERE id = ? AND user_id = ? AND deleted = 0")
                .bind(seq)
                .bind(id)
                .bind(user_id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
            if n == 0 {
                return Ok(Applied::Dropped);
            }
            sqlx::query("UPDATE progress_events SET deleted = 1, seq = ? WHERE reading_id = ?")
                .bind(seq)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            Ok(Applied::Yes)
        }
        Progress {
            event_id,
            reading_id,
            base,
            from,
            to,
            local_date,
        } => {
            check_date(local_date)?;
            check_pages(*from)?;
            check_pages(*to)?;
            let _ = base;
            if !move_position(conn, user_id, reading_id, chain, hlc, *to, seq).await? {
                return Ok(Applied::Dropped);
            }
            sqlx::query(
                "INSERT INTO progress_events (id, reading_id, user_id, hlc, local_date, from_page, to_page, seq)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
            )
            .bind(event_id)
            .bind(reading_id)
            .bind(user_id)
            .bind(hlc)
            .bind(local_date)
            .bind(from)
            .bind(to)
            .bind(seq)
            .execute(&mut *conn)
            .await?;
            Ok(Applied::Yes)
        }
        ProgressUndo {
            reading_id,
            event_id,
            base,
            to,
        } => {
            check_pages(*to)?;
            let _ = base;
            if !move_position(conn, user_id, reading_id, chain, hlc, *to, seq).await? {
                return Ok(Applied::Dropped);
            }
            sqlx::query(
                "UPDATE progress_events SET deleted = 1, seq = ? WHERE id = ? AND reading_id = ?",
            )
            .bind(seq)
            .bind(event_id)
            .bind(reading_id)
            .execute(&mut *conn)
            .await?;
            Ok(Applied::Yes)
        }
        WorkCreate { work } => create_private_work(conn, user_id, seq, work).await,
    }
}

/// The fork-aware position rule. Returns false if the move loses.
async fn move_position(
    conn: &mut SqliteConnection,
    user_id: &str,
    reading_id: &str,
    chain: Option<ChainCtx<'_>>,
    hlc: &str,
    to: i64,
    seq: i64,
) -> ApiResult<bool> {
    let Some(ChainCtx { info, outcomes }) = chain else {
        return Err(ApiError::internal(
            "progress operation without chain context",
        ));
    };
    let row: Option<(String, bool, String)> =
        sqlx::query_as("SELECT user_id, deleted, pos_clock FROM readings WHERE id = ?")
            .bind(reading_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((owner, deleted, current)) = row else {
        return Ok(false);
    };
    if owner != user_id || deleted {
        return Ok(false);
    }
    if !info.first {
        // Later links follow the chain's verdict.
        if outcomes[info.chain] != Some(true) {
            return Ok(false);
        }
    } else {
        let decision = mau_core::progress::decide(&current, &info.root, &info.tip);
        outcomes[info.chain] = Some(decision != mau_core::progress::Decision::Lose);
        match decision {
            mau_core::progress::Decision::Lose => return Ok(false),
            mau_core::progress::Decision::Append => {}
            mau_core::progress::Decision::ReplaceAfter(fork) => {
                // Another device moved on from `fork` and lost: its events stop counting.
                sqlx::query("UPDATE progress_events SET deleted = 1, seq = ? WHERE reading_id = ? AND hlc > ? AND deleted = 0")
                .bind(seq)
                .bind(reading_id)
                .bind(fork)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    sqlx::query("UPDATE readings SET pos = ?, pos_clock = ?, seq = ? WHERE id = ?")
        .bind(to)
        .bind(hlc)
        .bind(seq)
        .bind(reading_id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

pub fn clean_people(people: &[Person]) -> ApiResult<Vec<Person>> {
    let out: Vec<Person> = people
        .iter()
        .map(|p| Person {
            name: p.name.trim().to_string(),
            role: p.role.clone(),
        })
        .filter(|p| !p.name.is_empty())
        .collect();
    if out.len() > 30 {
        return Err(bad("too many people"));
    }
    for p in &out {
        if !["author", "translator", "editor", "illustrator"].contains(&p.role.as_str())
            || p.name.chars().count() > 200
        {
            return Err(bad("bad person"));
        }
    }
    Ok(out)
}

async fn create_private_work(
    conn: &mut SqliteConnection,
    user_id: &str,
    seq: i64,
    work: &NewWork,
) -> ApiResult<Applied> {
    let title = clean_text(&work.title, 300, "title")?;
    if title.is_empty() {
        return Err(bad("A title is required."));
    }
    let people = clean_people(&work.people)?;
    if !people.iter().any(|p| p.role == "author") {
        return Err(bad("At least one author is required."));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE id = ?)")
        .bind(&work.id)
        .fetch_one(&mut *conn)
        .await?;
    if exists {
        return Ok(Applied::Dropped);
    }
    sqlx::query("INSERT INTO works (id, title, first_year, owner_id, source, created_at, seq) VALUES (?, ?, ?, ?, 'manual', ?, ?)")
        .bind(&work.id)
        .bind(&title)
        .bind(work.first_year)
        .bind(user_id)
        .bind(crate::util::now_ms())
        .bind(seq)
        .execute(&mut *conn)
        .await?;
    insert_people(conn, &work.id, &people).await?;
    if let Some(e) = &work.edition {
        insert_edition(conn, &work.id, Some(user_id), e, seq).await?;
    }
    Ok(Applied::Yes)
}

pub async fn insert_people(
    conn: &mut SqliteConnection,
    work_id: &str,
    people: &[Person],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM work_people WHERE work_id = ?")
        .bind(work_id)
        .execute(&mut *conn)
        .await?;
    for (i, p) in people.iter().enumerate() {
        sqlx::query("INSERT INTO work_people (work_id, position, name, role) VALUES (?, ?, ?, ?)")
            .bind(work_id)
            .bind(i as i64)
            .bind(&p.name)
            .bind(&p.role)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn insert_edition(
    conn: &mut SqliteConnection,
    work_id: &str,
    owner: Option<&str>,
    e: &NewEdition,
    seq: i64,
) -> ApiResult<()> {
    let isbn = match e.isbn13.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(
            mau_core::isbn::normalize(raw).ok_or_else(|| bad("That ISBN doesn't look right."))?,
        ),
        None => None,
    };
    let binding = e.binding.as_deref().filter(|b| FORMATS.contains(b));
    sqlx::query(
        "INSERT INTO editions (id, work_id, publisher, binding, year, isbn13, pages, owner_id, source, seq)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'manual', ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&e.id)
    .bind(work_id)
    .bind(e.publisher.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(binding)
    .bind(e.year)
    .bind(isbn)
    .bind(e.pages.filter(|p| (1..=MAX_PAGES).contains(p)))
    .bind(owner)
    .bind(seq)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pulling changes

#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct Changes {
    /// The change number to send as `since` next time.
    #[ts(type = "number")]
    pub seq: i64,
    pub books: Vec<BookRow>,
    pub tags: Vec<TagRow>,
    pub copies: Vec<CopyRow>,
    pub readings: Vec<ReadingRow>,
    pub events: Vec<EventRow>,
    pub works: Vec<WorkRow>,
}

type BookTuple = (
    String,
    String,
    String,
    bool,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    bool,
    String,
);

fn book_from(t: BookTuple) -> BookRow {
    BookRow {
        id: t.0,
        work_id: t.1,
        rank: t.2,
        active: t.3,
        rating: t.4,
        notes: t.5,
        title_override: t.6,
        authors_override: t.7.and_then(|a| serde_json::from_str(&a).ok()),
        cover_override: t.8,
        added_at: t.9,
        deleted: t.10,
        clock: parse_clock(&t.11),
    }
}

pub async fn pull(conn: &mut SqliteConnection, user_id: &str, since: i64) -> ApiResult<Changes> {
    let seq = current_seq(conn).await?;
    // A new device (`since` 0) has nothing to delete, so it gets live rows only.
    let books: Vec<BookTuple> = sqlx::query_as(
        "SELECT id, work_id, rank, active, rating, notes, title_override, authors_override, cover_override, added_at, deleted, clock
         FROM books WHERE user_id = ? AND seq > ? AND (? OR deleted = 0)",
    )
    .bind(user_id)
    .bind(since)
    .bind(since > 0)
    .fetch_all(&mut *conn)
    .await?;
    let books: Vec<BookRow> = books.into_iter().map(book_from).collect();

    let tags: Vec<(String, String, bool, String)> = sqlx::query_as(
        "SELECT book_id, tag, present, clock FROM book_tags WHERE user_id = ? AND seq > ? AND (? OR present = 1)",
    )
    .bind(user_id)
    .bind(since)
    .bind(since > 0)
    .fetch_all(&mut *conn)
    .await?;
    let copies: Vec<(String, String, String, String, Option<String>, bool, String)> = sqlx::query_as(
        "SELECT id, book_id, format, location, edition_id, deleted, clock FROM copies WHERE user_id = ? AND seq > ? AND (? OR deleted = 0)",
    )
    .bind(user_id)
    .bind(since)
    .bind(since > 0)
    .fetch_all(&mut *conn)
    .await?;
    let readings: Vec<(
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
        String,
        bool,
        String,
    )> = sqlx::query_as(
        "SELECT id, book_id, started_on, finished_on, total, pos, pos_clock, deleted, clock
         FROM readings WHERE user_id = ? AND seq > ? AND (? OR deleted = 0)",
    )
    .bind(user_id)
    .bind(since)
    .bind(since > 0)
    .fetch_all(&mut *conn)
    .await?;
    let events: Vec<(String, String, String, String, i64, i64, bool)> = sqlx::query_as(
        "SELECT id, reading_id, hlc, local_date, from_page, to_page, deleted FROM progress_events WHERE user_id = ? AND seq > ? AND (? OR deleted = 0)",
    )
    .bind(user_id)
    .bind(since)
    .bind(since > 0)
    .fetch_all(&mut *conn)
    .await?;

    // Works: those of changed books, plus catalog edits to works on the list
    // (found from the edits, by index, not by reading the whole list). A new
    // device gets every book, so their works are all there already.
    let mut work_ids: HashSet<String> = books.iter().map(|b| b.work_id.clone()).collect();
    if since > 0 {
        let edited: Vec<String> = sqlx::query_scalar(
            "SELECT c.id FROM (
                 SELECT id FROM works WHERE seq > ?2
                 UNION ALL SELECT work_id FROM editions WHERE seq > ?2
             ) c WHERE EXISTS(SELECT 1 FROM books b WHERE b.work_id = c.id AND b.user_id = ?1 AND b.deleted = 0)",
        )
        .bind(user_id)
        .bind(since)
        .fetch_all(&mut *conn)
        .await?;
        work_ids.extend(edited);
    }
    let work_ids: Vec<String> = work_ids.into_iter().collect();
    let works = load_works(conn, &work_ids, Some(user_id)).await?;

    Ok(Changes {
        seq,
        books,
        tags: tags
            .into_iter()
            .map(|(book_id, tag, present, clock)| TagRow {
                book_id,
                tag,
                present,
                clock,
            })
            .collect(),
        copies: copies
            .into_iter()
            .map(
                |(id, book_id, format, location, edition_id, deleted, clock)| CopyRow {
                    id,
                    book_id,
                    format,
                    location,
                    edition_id,
                    deleted,
                    clock: parse_clock(&clock),
                },
            )
            .collect(),
        readings: readings
            .into_iter()
            .map(
                |(id, book_id, started_on, finished_on, total, pos, pos_clock, deleted, clock)| {
                    ReadingRow {
                        id,
                        book_id,
                        started_on,
                        finished_on,
                        total,
                        pos,
                        pos_clock,
                        deleted,
                        clock: parse_clock(&clock),
                    }
                },
            )
            .collect(),
        events: events
            .into_iter()
            .map(
                |(id, reading_id, hlc, local_date, from_page, to_page, deleted)| EventRow {
                    id,
                    reading_id,
                    hlc,
                    local_date,
                    from_page,
                    to_page,
                    deleted,
                },
            )
            .collect(),
        works,
    })
}

/// Loads a work with its people and editions. Private works and editions
/// are only returned to their owner (`viewer`).
pub async fn load_work(
    conn: &mut SqliteConnection,
    id: &str,
    viewer: Option<&str>,
) -> sqlx::Result<Option<WorkRow>> {
    Ok(load_works(conn, &[id.to_owned()], viewer).await?.pop())
}

type EditionTuple = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
);

/// Loads works like `load_work`, in three queries however many there are
/// (a new device's first sync asks for its whole library). Works it can't
/// see are left out; the order is the database's.
pub async fn load_works(
    conn: &mut SqliteConnection,
    ids: &[String],
    viewer: Option<&str>,
) -> sqlx::Result<Vec<WorkRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = serde_json::to_string(ids).expect("strings serialize");
    let viewer = viewer.unwrap_or("");
    let works: Vec<(
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT id, title, first_year, cover, owner_id, suggested FROM works
             WHERE id IN (SELECT value FROM json_each(?1)) AND (owner_id IS NULL OR owner_id = ?2)",
    )
    .bind(&ids)
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;
    let people: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT work_id, name, role FROM work_people
         WHERE work_id IN (SELECT value FROM json_each(?)) ORDER BY work_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let editions: Vec<EditionTuple> = sqlx::query_as(
        "SELECT work_id, id, publisher, binding, year, isbn13, pages, cover, owner_id FROM editions
         WHERE work_id IN (SELECT value FROM json_each(?1)) AND (owner_id IS NULL OR owner_id = ?2)
         ORDER BY work_id, year IS NULL, year, id",
    )
    .bind(&ids)
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;

    let mut people_of: HashMap<String, Vec<Person>> = HashMap::new();
    for (work_id, name, role) in people {
        people_of
            .entry(work_id)
            .or_default()
            .push(Person { name, role });
    }
    let mut editions_of: HashMap<String, Vec<EditionRow>> = HashMap::new();
    for (work_id, id, publisher, binding, year, isbn13, pages, cover, owner) in editions {
        editions_of
            .entry(work_id.clone())
            .or_default()
            .push(EditionRow {
                id,
                work_id,
                publisher,
                binding,
                year,
                isbn13,
                pages,
                cover,
                private: owner.is_some(),
            });
    }
    Ok(works
        .into_iter()
        .map(|(id, title, first_year, cover, owner, suggested)| WorkRow {
            title,
            first_year,
            cover,
            private: owner.is_some(),
            suggested,
            people: people_of.remove(&id).unwrap_or_default(),
            editions: editions_of.remove(&id).unwrap_or_default(),
            id,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Endpoint

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SyncRequest {
    #[ts(type = "number")]
    pub since: i64,
    pub ops: Vec<Op>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SyncResponse {
    /// IDs of operations that were ignored (stale or about deleted rows).
    pub dropped: Vec<String>,
    /// Operations rejected as invalid, with the reason. Devices discard them.
    pub rejected: Vec<Rejected>,
    pub changes: Changes,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Rejected {
    pub id: String,
    pub message: String,
}

const MAX_OPS: usize = 500;

/// Chain information for each progress operation in a batch (others: `None`),
/// and a fresh outcome table.
pub fn progress_chains(
    ops: &[Op],
) -> (
    Vec<Option<mau_core::progress::ChainInfo>>,
    Vec<Option<bool>>,
) {
    let changes: Vec<mau_core::progress::Change> = ops
        .iter()
        .filter_map(|op| match &op.op {
            OpKind::Progress {
                reading_id, base, ..
            }
            | OpKind::ProgressUndo {
                reading_id, base, ..
            } => Some(mau_core::progress::Change {
                reading: reading_id,
                base,
                hlc: &op.hlc,
            }),
            _ => None,
        })
        .collect();
    let infos = mau_core::progress::chains(&changes);
    let n = infos.iter().map(|c| c.chain + 1).max().unwrap_or(0);
    let mut infos = infos.into_iter();
    let per_op = ops
        .iter()
        .map(|op| match op.op {
            OpKind::Progress { .. } | OpKind::ProgressUndo { .. } => infos.next(),
            _ => None,
        })
        .collect();
    (per_op, vec![None; n])
}

pub async fn sync(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<SyncRequest>,
) -> ApiResult<Json<SyncResponse>> {
    if req.ops.len() > MAX_OPS {
        return Err(ApiError::bad_request(
            "too_many_ops",
            format!("Send at most {MAX_OPS} operations at a time."),
        ));
    }
    let mut tx = state.db.begin().await?;
    let mut dropped = Vec::new();
    let mut rejected = Vec::new();
    if !req.ops.is_empty() {
        let seq = next_seq(&mut tx).await?;
        let (chain_infos, mut outcomes) = progress_chains(&req.ops);
        for (i, op) in req.ops.iter().enumerate() {
            let ctx = chain_infos[i].as_ref().map(|info| ChainCtx {
                info,
                outcomes: &mut outcomes,
            });
            // Each operation runs in a savepoint so one bad operation can't
            // leave half its changes behind.
            sqlx::query("SAVEPOINT op").execute(&mut *tx).await?;
            match apply(&mut tx, &s.user.id, seq, op, ctx).await {
                Ok(Applied::Yes) => {}
                Ok(Applied::Dropped) => dropped.push(op.id.clone()),
                Err(e) if e.status.is_client_error() => {
                    sqlx::query("ROLLBACK TO op").execute(&mut *tx).await?;
                    rejected.push(Rejected {
                        id: op.id.clone(),
                        message: e.message,
                    });
                }
                Err(e) => return Err(e),
            }
            sqlx::query("RELEASE op").execute(&mut *tx).await?;
        }
    }
    let changes = pull(&mut tx, &s.user.id, req.since).await?;
    tx.commit().await?;
    Ok(Json(SyncResponse {
        dropped,
        rejected,
        changes,
    }))
}

/// Deletes what a user owns that foreign-key cascades don't reach.
/// (Books, readings, private works, etc. all cascade from `users`.)
pub async fn delete_user_data(
    _tx: &mut Transaction<'_, Sqlite>,
    _user_id: &str,
) -> sqlx::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
