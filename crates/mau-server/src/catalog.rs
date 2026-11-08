//! Catalog endpoints: provider search, ISBN lookup, editions, adding a found
//! work to the shared catalog, similar-title checks, and suggestions.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppState;
use crate::auth::session::Authed;
use crate::error::{ApiError, ApiResult};
use crate::library::{self, Person, WorkRow};
use crate::providers::{self, Candidate, CandidateEdition, ProviderConfig};
use crate::util::{new_id, now_ms};

/// Fewer results than this from one provider counts as "thin": the next
/// provider is asked too.
const THIN: usize = 3;
const MAX_RESULTS: usize = 20;
/// Editions imported with a work (besides the one asked for).
const MAX_EDITIONS: usize = 20;

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SearchResult {
    #[serde(flatten)]
    pub candidate: Candidate,
    /// The user's book for this work, if it's already on their list.
    pub book_id: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SearchResponse {
    /// The normalized ISBN-13 when the query was an ISBN.
    pub isbn: Option<String>,
    pub results: Vec<SearchResult>,
    /// Providers that contributed results, in order (display names).
    pub sources: Vec<String>,
    /// Providers that failed, with the reason.
    pub errors: Vec<String>,
}

fn dedupe_key(c: &Candidate) -> String {
    let t: String = c
        .title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let a: String = c
        .authors
        .first()
        .map(|a| a.to_lowercase())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    format!("{t}|{a}")
}

pub(crate) async fn cache_candidate(state: &AppState, c: &Candidate) {
    providers::cache_put(
        &state.db,
        &format!("cand:{}:{}", c.provider, c.source_id),
        c,
    )
    .await;
}

pub(crate) async fn provider_search(
    state: &AppState,
    p: &ProviderConfig,
    q: &str,
) -> Result<Vec<Candidate>, String> {
    let key = format!("search:{}:{}", p.name.id(), q.to_lowercase());
    if let Some(hit) = providers::cache_get(&state.db, &key).await {
        return Ok(hit);
    }
    let res = providers::search(&state.http, p, q).await?;
    providers::cache_put(&state.db, &key, &res).await;
    Ok(res)
}

pub(crate) async fn provider_isbn(
    state: &AppState,
    p: &ProviderConfig,
    isbn: &str,
) -> Result<Option<Candidate>, String> {
    let key = format!("isbn:{}:{isbn}", p.name.id());
    if let Some(hit) = providers::cache_get::<Option<Candidate>>(&state.db, &key).await {
        return Ok(hit);
    }
    let res = providers::by_isbn(&state.http, p, isbn).await?;
    providers::cache_put(&state.db, &key, &res).await;
    Ok(res)
}

/// The user's book for a work found by a provider (by source, or by ISBN).
async fn on_list(state: &AppState, user_id: &str, c: &Candidate) -> sqlx::Result<Option<String>> {
    let by_source: Option<String> = sqlx::query_scalar(
        "SELECT b.id FROM books b JOIN works w ON w.id = b.work_id
         WHERE b.user_id = ? AND b.deleted = 0 AND w.source = ? AND w.source_id = ? LIMIT 1",
    )
    .bind(user_id)
    .bind(&c.provider)
    .bind(&c.source_id)
    .fetch_optional(&state.db)
    .await?;
    if by_source.is_some() {
        return Ok(by_source);
    }
    for e in &c.editions {
        if let Some(isbn) = &e.isbn13 {
            let hit: Option<String> = sqlx::query_scalar(
                "SELECT b.id FROM books b JOIN editions e ON e.work_id = b.work_id
                 WHERE b.user_id = ? AND b.deleted = 0 AND e.isbn13 = ? LIMIT 1",
            )
            .bind(user_id)
            .bind(isbn)
            .fetch_optional(&state.db)
            .await?;
            if hit.is_some() {
                return Ok(hit);
            }
        }
    }
    Ok(None)
}

/// Asks enabled providers in order (see the module docs), returning
/// de-duplicated candidates, the providers that contributed, and failures.
pub async fn find_candidates(
    state: &AppState,
    query: &str,
    isbn: Option<&str>,
) -> ApiResult<(Vec<Candidate>, Vec<String>, Vec<String>)> {
    let mut results: Vec<Candidate> = Vec::new();
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for p in providers::enabled(&state.db).await? {
        let found = match isbn {
            Some(i) => provider_isbn(state, &p, i)
                .await
                .map(|c| c.into_iter().collect::<Vec<_>>()),
            None => provider_search(state, &p, query).await,
        };
        match found {
            Ok(list) => {
                let before = results.len();
                for c in list {
                    if seen.insert(dedupe_key(&c)) && results.len() < MAX_RESULTS {
                        results.push(c);
                    }
                }
                if results.len() > before {
                    sources.push(p.name.label().to_string());
                }
            }
            Err(e) => {
                tracing::warn!("catalog provider failed: {e}");
                errors.push(e);
            }
        }
        let enough = if isbn.is_some() {
            !results.is_empty()
        } else {
            results.len() >= THIN
        };
        if enough {
            break;
        }
    }
    for c in &results {
        cache_candidate(state, c).await;
    }
    Ok((results, sources, errors))
}

/// Searches only Google Books with a user's own key.
async fn own_google(
    state: &AppState,
    key: &str,
    query: &str,
    isbn: Option<&str>,
) -> (Vec<Candidate>, Vec<String>, Vec<String>) {
    let p = providers::own_google(key);
    let found = match isbn {
        Some(i) => provider_isbn(state, &p, i)
            .await
            .map(|c| c.into_iter().collect::<Vec<_>>()),
        None => provider_search(state, &p, query).await,
    };
    match found {
        Ok(mut list) => {
            list.truncate(MAX_RESULTS);
            for c in &list {
                cache_candidate(state, c).await;
            }
            let sources = if list.is_empty() {
                vec![]
            } else {
                vec![p.name.label().to_string()]
            };
            (list, sources, vec![])
        }
        Err(e) => (vec![], vec![], vec![e]),
    }
}

/// `GET /api/catalog/search?q=…`: with the user's own Google Books key, asks
/// only Google Books. Otherwise asks enabled providers in order, moving on
/// only when results are missing or thin.
pub async fn search(
    State(state): State<AppState>,
    Authed(s): Authed,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResponse>> {
    let query = q.q.trim();
    if query.is_empty() {
        return Err(ApiError::bad_request(
            "empty",
            "Type a title, author, or ISBN.",
        ));
    }
    let isbn = if mau_core::isbn::looks_like_isbn(query) {
        Some(mau_core::isbn::normalize(query).ok_or_else(|| {
            ApiError::bad_request(
                "bad_isbn",
                "That ISBN's check digit doesn't match. Check for a typo.",
            )
        })?)
    } else {
        None
    };
    state.limits.check("search", &s.user.id, 30)?;
    let (results, sources, errors) = match s.user.google_books_key.as_deref() {
        Some(key) => own_google(&state, key, query, isbn.as_deref()).await,
        None => find_candidates(&state, query, isbn.as_deref()).await?,
    };

    let mut out = Vec::with_capacity(results.len());
    for c in results {
        let book_id = on_list(&state, &s.user.id, &c).await?;
        out.push(SearchResult {
            candidate: c,
            book_id,
        });
    }
    Ok(Json(SearchResponse {
        isbn,
        results: out,
        sources,
        errors,
    }))
}

#[derive(Deserialize)]
pub struct EditionsQuery {
    provider: String,
    source_id: String,
}

async fn cached_candidate(
    state: &AppState,
    provider: &str,
    source_id: &str,
) -> ApiResult<Candidate> {
    providers::cache_get(&state.db, &format!("cand:{provider}:{source_id}"))
        .await
        .ok_or_else(|| ApiError::not_found("That search result has expired. Search again."))
}

async fn all_editions(state: &AppState, c: &Candidate) -> Vec<CandidateEdition> {
    let key = format!("editions:{}:{}", c.provider, c.source_id);
    if let Some(hit) = providers::cache_get(&state.db, &key).await {
        return hit;
    }
    match providers::editions(&state.http, c).await {
        Ok(list) => {
            providers::cache_put(&state.db, &key, &list).await;
            list
        }
        Err(e) => {
            tracing::warn!("fetching editions failed: {e}");
            c.editions.clone()
        }
    }
}

/// `GET /api/catalog/editions?provider=…&source_id=…`: editions to choose from
/// when adding a search result.
pub async fn editions(
    State(state): State<AppState>,
    Authed(_): Authed,
    Query(q): Query<EditionsQuery>,
) -> ApiResult<Json<Vec<CandidateEdition>>> {
    let c = cached_candidate(&state, &q.provider, &q.source_id).await?;
    let mut list = all_editions(&state, &c).await;
    merge_editions(&mut list, &c.editions);
    Ok(Json(list))
}

/// Adds the candidate's own editions (e.g. from an ISBN lookup, which has the
/// most detail) to a fetched list, filling gaps in matching entries.
fn merge_editions(list: &mut Vec<CandidateEdition>, own: &[CandidateEdition]) {
    for e in own {
        match list
            .iter_mut()
            .find(|x| x.isbn13.is_some() && x.isbn13 == e.isbn13)
        {
            Some(x) => {
                x.publisher = x.publisher.take().or_else(|| e.publisher.clone());
                x.binding = e.binding.clone().or_else(|| x.binding.take());
                x.year = x.year.or(e.year);
                x.pages = x.pages.or(e.pages);
                x.cover_url = x.cover_url.take().or_else(|| e.cover_url.clone());
            }
            None => list.insert(0, e.clone()),
        }
    }
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct EnsureRequest {
    pub provider: String,
    pub source_id: String,
    /// Make sure this edition is included (the scanned or chosen ISBN).
    pub isbn: Option<String>,
}

/// `POST /api/catalog/ensure`: puts a search result into the shared catalog
/// (or finds it there) and returns it, ready for `book.create`.
pub async fn ensure(
    State(state): State<AppState>,
    Authed(_): Authed,
    Json(req): Json<EnsureRequest>,
) -> ApiResult<Json<WorkRow>> {
    let c = cached_candidate(&state, &req.provider, &req.source_id).await?;
    Ok(Json(
        ensure_candidate(&state, &c, req.isbn.as_deref()).await?,
    ))
}

/// Finds or adds a provider's work in the shared catalog, with up to
/// `MAX_EDITIONS` editions and its cover.
pub async fn ensure_candidate(
    state: &AppState,
    c: &Candidate,
    isbn: Option<&str>,
) -> ApiResult<WorkRow> {
    let wanted_isbn = isbn.and_then(mau_core::isbn::normalize);

    // Already in the shared catalog, by source or by the wanted ISBN?
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM works WHERE owner_id IS NULL AND source = ? AND source_id = ?",
    )
    .bind(&c.provider)
    .bind(&c.source_id)
    .fetch_optional(&state.db)
    .await?;
    let existing = match (existing, &wanted_isbn) {
        (Some(id), _) => Some(id),
        (None, Some(isbn)) => {
            sqlx::query_scalar(
                "SELECT work_id FROM editions WHERE owner_id IS NULL AND isbn13 = ? LIMIT 1",
            )
            .bind(isbn)
            .fetch_optional(&state.db)
            .await?
        }
        _ => None,
    };

    let mut editions = all_editions(state, c).await;
    merge_editions(&mut editions, &c.editions);
    // Keep the wanted edition, then editions with an ISBN.
    editions.sort_by_key(|e| {
        (
            e.isbn13.is_none() || e.isbn13 != wanted_isbn,
            e.isbn13.is_none(),
        )
    });
    editions.truncate(MAX_EDITIONS);

    let cover = cover_for(state, c, &editions).await;

    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let work_id = match existing {
        Some(id) => {
            if let Some(cover) = &cover {
                sqlx::query("UPDATE works SET cover = COALESCE(cover, ?), seq = ? WHERE id = ?")
                    .bind(cover)
                    .bind(seq)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
            id
        }
        None => {
            let id = new_id();
            sqlx::query(
                "INSERT INTO works (id, title, first_year, cover, source, source_id, created_at, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&c.title)
            .bind(c.first_year)
            .bind(&cover)
            .bind(&c.provider)
            .bind(&c.source_id)
            .bind(now_ms())
            .bind(seq)
            .execute(&mut *tx)
            .await?;
            let people: Vec<Person> = c
                .authors
                .iter()
                .map(|a| Person {
                    name: a.clone(),
                    role: "author".into(),
                })
                .collect();
            library::insert_people(
                &mut tx,
                &id,
                &library::clean_people(&people).unwrap_or_default(),
            )
            .await?;
            id
        }
    };
    for e in &editions {
        if let Some(isbn) = &e.isbn13 {
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM editions WHERE owner_id IS NULL AND isbn13 = ?)",
            )
            .bind(isbn)
            .fetch_one(&mut *tx)
            .await?;
            if known {
                continue;
            }
        } else if e.source_id.is_some() {
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM editions WHERE work_id = ? AND source_id = ?)",
            )
            .bind(&work_id)
            .bind(&e.source_id)
            .fetch_one(&mut *tx)
            .await?;
            if known {
                continue;
            }
        }
        sqlx::query(
            "INSERT INTO editions (id, work_id, publisher, binding, year, isbn13, pages, source, source_id, seq)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&work_id)
        .bind(&e.publisher)
        .bind(&e.binding)
        .bind(e.year)
        .bind(&e.isbn13)
        .bind(e.pages)
        .bind(&c.provider)
        .bind(&e.source_id)
        .bind(seq)
        .execute(&mut *tx)
        .await?;
    }
    let work = library::load_work(&mut tx, &work_id, None)
        .await?
        .ok_or_else(|| ApiError::internal("work vanished"))?;
    tx.commit().await?;
    Ok(work)
}

/// Google Books covers are small (about 128 px wide), so for its results
/// Open Library's large cover for one of the ISBNs comes first.
async fn cover_for(
    state: &AppState,
    c: &Candidate,
    editions: &[CandidateEdition],
) -> Option<String> {
    if c.provider == "google" {
        for isbn in editions.iter().filter_map(|e| e.isbn13.as_deref()).take(2) {
            let url = format!("https://covers.openlibrary.org/b/isbn/{isbn}-L.jpg?default=false");
            if let Some(hash) = crate::covers::fetch_and_store(state, &url).await {
                return Some(hash);
            }
        }
    }
    match &c.cover_url {
        Some(url) => crate::covers::fetch_and_store(state, url).await,
        None => None,
    }
}

/// `GET /api/catalog/works/{id}`: a shared work, or the user's own private one.
pub async fn work(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkRow>> {
    let mut conn = state.db.acquire().await?;
    library::load_work(&mut conn, &id, Some(&s.user.id))
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("No such book in the catalog."))
}

#[derive(Deserialize)]
pub struct SimilarQuery {
    title: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SimilarWork {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
    pub cover: Option<String>,
}

fn title_words(t: &str) -> (String, Vec<String>) {
    let lower = t.to_lowercase();
    let stripped = ["the ", "a ", "an "]
        .iter()
        .find_map(|a| lower.strip_prefix(a))
        .unwrap_or(&lower)
        .trim()
        .to_string();
    let words = stripped
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 5)
        .map(String::from)
        .collect();
    (stripped, words)
}

/// Whether two titles are similar enough to ask "Is it one of these?".
pub fn similar_titles(a: &str, b: &str) -> bool {
    let (sa, wa) = title_words(a);
    let (sb, wb) = title_words(b);
    if sa.is_empty() || sb.is_empty() {
        return false;
    }
    sa.contains(&sb) || sb.contains(&sa) || wa.iter().any(|w| wb.contains(w))
}

/// `GET /api/catalog/similar?title=…`: shared works with similar titles,
/// checked before saving a manual entry.
pub async fn similar(
    State(state): State<AppState>,
    Authed(_): Authed,
    Query(q): Query<SimilarQuery>,
) -> ApiResult<Json<Vec<SimilarWork>>> {
    let (stripped, words) = title_words(&q.title);
    if stripped.is_empty() {
        return Ok(Json(vec![]));
    }
    let mut patterns = vec![format!("%{stripped}%")];
    patterns.extend(words.iter().map(|w| format!("%{w}%")));
    let mut ids: Vec<String> = Vec::new();
    for p in patterns.iter().take(6) {
        let found: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, title FROM works WHERE owner_id IS NULL AND lower(title) LIKE ? LIMIT 20",
        )
        .bind(p)
        .fetch_all(&state.db)
        .await?;
        for (id, title) in found {
            if !ids.contains(&id) && similar_titles(&q.title, &title) {
                ids.push(id);
            }
        }
    }
    let mut out = Vec::new();
    let mut conn = state.db.acquire().await?;
    for id in ids.into_iter().take(5) {
        if let Some(w) = library::load_work(&mut conn, &id, None).await? {
            out.push(SimilarWork {
                authors: w
                    .people
                    .iter()
                    .filter(|p| p.role == "author")
                    .map(|p| p.name.clone())
                    .collect(),
                id: w.id,
                title: w.title,
                first_year: w.first_year,
                cover: w.cover,
            });
        }
    }
    Ok(Json(out))
}

/// `POST /api/catalog/works/{id}/suggest`: the owner of a manual entry asks
/// for it to be added to the shared catalog.
pub async fn suggest(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<crate::auth::handlers::Ok_>> {
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let n = sqlx::query(
        "UPDATE works SET suggested = 1, seq = ? WHERE id = ? AND owner_id = ? AND suggested = 0",
    )
    .bind(seq)
    .bind(&id)
    .bind(&s.user.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found(
            "Only your own manual entries can be suggested.",
        ));
    }
    tx.commit().await?;
    Ok(crate::auth::handlers::ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_title_rules() {
        assert!(similar_titles("Earthsea", "A Wizard of Earthsea"));
        assert!(similar_titles("The Lathe", "The Lathe of Heaven"));
        assert!(similar_titles(
            "Darkness Within",
            "The Left Hand of Darkness"
        ));
        assert!(!similar_titles(
            "Something else",
            "The Left Hand of Darkness"
        ));
    }
}
