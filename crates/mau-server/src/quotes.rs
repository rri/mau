//! A random line of public-domain science fiction for the landing page.
//! The list and how it was chosen: `data/README.md`.

use std::sync::LazyLock;

use axum::Json;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use rand::Rng;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct Quote {
    pub text: String,
    pub author: String,
    pub title: String,
    #[ts(type = "number | null")]
    pub year: Option<i32>,
    /// Project Gutenberg ebook number.
    #[ts(type = "number")]
    pub gutenberg: u32,
}

static QUOTES: LazyLock<Vec<Quote>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/quotes.json")).expect("data/quotes.json is valid")
});

/// `GET /api/quote`: a different one on every load, so never cached.
pub async fn random() -> Response {
    let q = &QUOTES[rand::rng().random_range(0..QUOTES.len())];
    let mut res = Json(q).into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_a_thousand_well_formed_quotes() {
        assert!(QUOTES.len() >= 1000, "{}", QUOTES.len());
        for q in QUOTES.iter() {
            assert!(!q.text.is_empty() && !q.author.is_empty() && !q.title.is_empty());
            assert!(q.text.chars().count() <= 240, "{}", q.text);
            assert!(q.year.is_none_or(|y| (1600..=1955).contains(&y)), "{q:?}");
        }
    }
}
