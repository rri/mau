//! Exporting a library: full JSON (re-importable into mau) or a simple CSV.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::session::Authed;
use crate::error::ApiResult;
use crate::library::{self, Changes, Person};

pub const FORMAT: &str = "mau-export";
pub const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub account: Account,
    pub books: Vec<Book>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub time_zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub title: String,
    pub people: Vec<Person>,
    pub first_year: Option<i64>,
    pub editions: Vec<Edition>,
    /// Your own edits to title, authors.
    pub title_override: Option<String>,
    pub authors_override: Option<Vec<String>>,
    pub active: bool,
    pub rank: String,
    pub rating: i64,
    pub notes: String,
    pub tags: Vec<String>,
    pub added_at: i64,
    pub copies: Vec<Copy>,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edition {
    pub publisher: Option<String>,
    pub binding: Option<String>,
    pub year: Option<i64>,
    pub isbn13: Option<String>,
    pub pages: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copy {
    pub format: String,
    pub location: String,
    pub edition_isbn13: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub started_on: String,
    pub finished_on: Option<String>,
    pub total: i64,
    pub pos: i64,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub date: String,
    pub from: i64,
    pub to: i64,
}

pub async fn build(state: &AppState, user: &crate::auth::User) -> ApiResult<Export> {
    let mut conn = state.db.acquire().await?;
    let c: Changes = library::pull(&mut conn, &user.id, 0).await?;
    let works: HashMap<_, _> = c.works.iter().map(|w| (w.id.clone(), w)).collect();
    let mut books = Vec::new();
    for b in c.books.iter().filter(|b| !b.deleted) {
        let Some(w) = works.get(&b.work_id) else {
            continue;
        };
        let editions: HashMap<_, _> = w.editions.iter().map(|e| (e.id.clone(), e)).collect();
        let mut readings: Vec<_> = c
            .readings
            .iter()
            .filter(|r| r.book_id == b.id && !r.deleted)
            .collect();
        readings.sort_by(|a, b| a.id.cmp(&b.id));
        books.push(Book {
            title: w.title.clone(),
            people: w.people.clone(),
            first_year: w.first_year,
            editions: w
                .editions
                .iter()
                .map(|e| Edition {
                    publisher: e.publisher.clone(),
                    binding: e.binding.clone(),
                    year: e.year,
                    isbn13: e.isbn13.clone(),
                    pages: e.pages,
                })
                .collect(),
            title_override: b.title_override.clone(),
            authors_override: b.authors_override.clone(),
            active: b.active,
            rank: b.rank.clone(),
            rating: b.rating,
            notes: b.notes.clone(),
            tags: c
                .tags
                .iter()
                .filter(|t| t.book_id == b.id && t.present)
                .map(|t| t.tag.clone())
                .collect(),
            added_at: b.added_at,
            copies: c
                .copies
                .iter()
                .filter(|x| x.book_id == b.id && !x.deleted)
                .map(|x| Copy {
                    format: x.format.clone(),
                    location: x.location.clone(),
                    edition_isbn13: x
                        .edition_id
                        .as_ref()
                        .and_then(|id| editions.get(id))
                        .and_then(|e| e.isbn13.clone()),
                })
                .collect(),
            readings: readings
                .into_iter()
                .map(|r| {
                    let mut events: Vec<_> = c
                        .events
                        .iter()
                        .filter(|e| e.reading_id == r.id && !e.deleted)
                        .collect();
                    events.sort_by(|a, b| a.hlc.cmp(&b.hlc));
                    Reading {
                        started_on: r.started_on.clone(),
                        finished_on: r.finished_on.clone(),
                        total: r.total,
                        pos: r.pos,
                        events: events
                            .into_iter()
                            .map(|e| Event {
                                date: e.local_date.clone(),
                                from: e.from_page,
                                to: e.to_page,
                            })
                            .collect(),
                    }
                })
                .collect(),
        });
    }
    books.sort_by(|a, b| a.rank.cmp(&b.rank));
    Ok(Export {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: crate::util::date_string(crate::util::today_days()),
        account: Account {
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            bio: user.bio.clone(),
            time_zone: user.time_zone.clone(),
        },
        books,
    })
}

fn download(body: Vec<u8>, content_type: &'static str, filename: String) -> Response {
    let mut res = body.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

pub async fn json(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Response> {
    let export = build(&state, &s.user).await?;
    let name = format!("mau-{}-{}.json", s.user.username, export.exported_at);
    Ok(download(
        serde_json::to_vec_pretty(&export)?,
        "application/json",
        name,
    ))
}

pub async fn csv(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Response> {
    let export = build(&state, &s.user).await?;
    let mut w = csv::Writer::from_writer(Vec::new());
    let header = [
        "Title",
        "Authors",
        "State",
        "Current page",
        "Total pages",
        "Started",
        "Finished",
        "Rating",
        "Tags",
        "Owned",
        "Copies",
        "ISBN",
        "Notes",
    ];
    w.write_record(header)
        .map_err(crate::error::ApiError::internal)?;
    for b in &export.books {
        let authors = b.authors_override.clone().unwrap_or_else(|| {
            b.people
                .iter()
                .filter(|p| p.role == "author")
                .map(|p| p.name.clone())
                .collect()
        });
        let r = b.readings.last();
        let state = match r {
            Some(r) if r.finished_on.is_some() => "Finished",
            _ if b.active => "Active",
            _ => "Inactive",
        };
        let finished = r
            .and_then(|r| r.finished_on.clone())
            .filter(|d| d != crate::importer::UNKNOWN_DATE)
            .unwrap_or_default();
        let copies = b
            .copies
            .iter()
            .map(|c| {
                if c.location.is_empty() {
                    c.format.clone()
                } else {
                    format!("{} ({})", c.format, c.location)
                }
            })
            .collect::<Vec<_>>();
        w.write_record([
            b.title_override.clone().unwrap_or_else(|| b.title.clone()),
            authors.join(", "),
            state.into(),
            r.map(|r| r.pos.to_string()).unwrap_or_default(),
            r.map(|r| r.total.to_string()).unwrap_or_default(),
            r.map(|r| r.started_on.clone())
                .filter(|d| d != crate::importer::UNKNOWN_DATE)
                .unwrap_or_default(),
            finished,
            if b.rating > 0 {
                b.rating.to_string()
            } else {
                String::new()
            },
            b.tags.join(", "),
            if b.copies.is_empty() {
                "No".into()
            } else {
                "Yes".into()
            },
            copies.join("; "),
            b.editions
                .iter()
                .find_map(|e| e.isbn13.clone())
                .unwrap_or_default(),
            b.notes.clone(),
        ])
        .map_err(crate::error::ApiError::internal)?;
    }
    let bytes = w.into_inner().map_err(crate::error::ApiError::internal)?;
    let name = format!("mau-{}-{}.csv", s.user.username, export.exported_at);
    Ok(download(bytes, "text/csv; charset=utf-8", name))
}
