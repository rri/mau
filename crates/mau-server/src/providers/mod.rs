//! Catalog providers: Open Library (built in), Google Books, Hardcover, and
//! ISBNdb. Each maps its results to [`Candidate`]s.

pub mod google;
pub mod hardcover;
pub mod isbndb;
pub mod openlibrary;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::util::{DAY_MS, now_ms};

/// A work found by a provider.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub provider: String,
    pub source_id: String,
    pub title: String,
    pub authors: Vec<String>,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
    pub cover_url: Option<String>,
    #[ts(type = "number | null")]
    pub edition_count: Option<i64>,
    /// Editions the provider returned with the work (may be empty until fetched).
    pub editions: Vec<CandidateEdition>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CandidateEdition {
    pub source_id: Option<String>,
    pub publisher: Option<String>,
    pub binding: Option<String>,
    #[ts(type = "number | null")]
    pub year: Option<i64>,
    pub isbn13: Option<String>,
    #[ts(type = "number | null")]
    pub pages: Option<i64>,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ProviderName {
    Openlibrary,
    Google,
    Hardcover,
    Isbndb,
}

impl ProviderName {
    pub fn label(self) -> &'static str {
        match self {
            ProviderName::Openlibrary => "Open Library",
            ProviderName::Google => "Google Books",
            ProviderName::Hardcover => "Hardcover",
            ProviderName::Isbndb => "ISBNdb",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            ProviderName::Openlibrary => "openlibrary",
            ProviderName::Google => "google",
            ProviderName::Hardcover => "hardcover",
            ProviderName::Isbndb => "isbndb",
        }
    }
    pub fn needs_key(self) -> bool {
        !matches!(self, ProviderName::Openlibrary)
    }
}

/// One entry in the admin's ordered provider list.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfig {
    pub name: ProviderName,
    pub enabled: bool,
    /// API key; never sent to browsers (see `ProviderInfo`).
    pub key: Option<String>,
}

pub fn default_providers() -> Vec<ProviderConfig> {
    use ProviderName::*;
    [Openlibrary, Google, Hardcover, Isbndb]
        .into_iter()
        .map(|name| ProviderConfig {
            name,
            enabled: name == Openlibrary,
            key: None,
        })
        .collect()
}

/// Google Books with a user's own key.
pub fn own_google(key: &str) -> ProviderConfig {
    ProviderConfig {
        name: ProviderName::Google,
        enabled: true,
        key: Some(key.to_string()),
    }
}

pub async fn configured(db: &SqlitePool) -> sqlx::Result<Vec<ProviderConfig>> {
    Ok(crate::settings::get(db, "providers")
        .await?
        .unwrap_or_else(default_providers))
}

/// Enabled providers in order (keyed providers only when they have a key).
pub async fn enabled(db: &SqlitePool) -> sqlx::Result<Vec<ProviderConfig>> {
    Ok(configured(db)
        .await?
        .into_iter()
        .filter(|p| {
            p.enabled && (!p.name.needs_key() || p.key.as_deref().is_some_and(|k| !k.is_empty()))
        })
        .collect())
}

pub type ProviderResult<T> = Result<T, String>;

pub async fn search(
    http: &reqwest::Client,
    p: &ProviderConfig,
    q: &str,
) -> ProviderResult<Vec<Candidate>> {
    let key = p.key.as_deref().unwrap_or("");
    match p.name {
        ProviderName::Openlibrary => openlibrary::search(http, q).await,
        ProviderName::Google => google::search(http, key, q).await,
        ProviderName::Hardcover => hardcover::search(http, key, q).await,
        ProviderName::Isbndb => isbndb::search(http, key, q).await,
    }
}

pub async fn by_isbn(
    http: &reqwest::Client,
    p: &ProviderConfig,
    isbn: &str,
) -> ProviderResult<Option<Candidate>> {
    let key = p.key.as_deref().unwrap_or("");
    match p.name {
        ProviderName::Openlibrary => openlibrary::by_isbn(http, isbn).await,
        ProviderName::Google => google::by_isbn(http, key, isbn).await,
        ProviderName::Hardcover => hardcover::by_isbn(http, key, isbn).await,
        ProviderName::Isbndb => isbndb::by_isbn(http, key, isbn).await,
    }
}

/// All editions of a work, for providers that group editions (Open Library).
/// Others return the candidate's own editions.
pub async fn editions(
    http: &reqwest::Client,
    c: &Candidate,
) -> ProviderResult<Vec<CandidateEdition>> {
    match c.provider.as_str() {
        "openlibrary" => openlibrary::editions(http, &c.source_id).await,
        _ => Ok(c.editions.clone()),
    }
}

// ---------------------------------------------------------------------------
// Response cache

const CACHE_TTL_MS: i64 = 30 * DAY_MS;

pub async fn cache_get<T: serde::de::DeserializeOwned>(db: &SqlitePool, key: &str) -> Option<T> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT value FROM provider_cache WHERE key = ? AND fetched_at > ?")
            .bind(key)
            .bind(now_ms() - CACHE_TTL_MS)
            .fetch_optional(db)
            .await
            .ok()?;
    serde_json::from_str(&row?).ok()
}

pub async fn cache_put<T: Serialize>(db: &SqlitePool, key: &str, value: &T) {
    let _ = sqlx::query(
        "INSERT INTO provider_cache (key, value, fetched_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(serde_json::to_string(value).unwrap_or_default())
    .bind(now_ms())
    .execute(db)
    .await;
}

// ---------------------------------------------------------------------------
// Helpers shared by providers

/// The first four-digit year in a string like "March 1975" or "1975-03-01".
pub fn year_of(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(3)).find_map(|i| {
        let w = &b[i..i + 4];
        let boundary_before = i == 0 || !b[i - 1].is_ascii_digit();
        let boundary_after = i + 4 == b.len() || !b[i + 4].is_ascii_digit();
        (w.iter().all(u8::is_ascii_digit) && boundary_before && boundary_after)
            .then(|| std::str::from_utf8(w).unwrap().parse().unwrap())
            .filter(|y| (1000..=2100).contains(y))
    })
}

/// Maps a provider's format text to mau's bindings.
pub fn binding_of(s: &str) -> Option<String> {
    let s = s.to_ascii_lowercase();
    let b = if s.contains("hard") {
        "Hardcover"
    } else if s.contains("paper") || s.contains("mass market") || s.contains("trade") {
        "Paperback"
    } else if s.contains("ebook")
        || s.contains("e-book")
        || s.contains("kindle")
        || s.contains("digital")
    {
        "Ebook"
    } else if s.contains("audio") {
        "Audiobook"
    } else {
        return None;
    };
    Some(b.to_string())
}

pub fn https(url: &str) -> String {
    url.strip_prefix("http://")
        .map_or_else(|| url.to_string(), |rest| format!("https://{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn years() {
        assert_eq!(year_of("March 1975"), Some(1975));
        assert_eq!(year_of("1975-03-01"), Some(1975));
        assert_eq!(year_of("c2003"), Some(2003));
        assert_eq!(year_of("12345"), None);
        assert_eq!(year_of(""), None);
    }

    #[test]
    fn bindings() {
        assert_eq!(
            binding_of("Mass Market Paperback").as_deref(),
            Some("Paperback")
        );
        assert_eq!(binding_of("Hardcover").as_deref(), Some("Hardcover"));
        assert_eq!(binding_of("Audio CD").as_deref(), Some("Audiobook"));
        assert_eq!(binding_of("Unknown"), None);
    }
}
