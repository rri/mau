//! ISBNdb (isbndb.com): paid; needs an API key. Results are editions.

use serde_json::Value;

use super::{Candidate, CandidateEdition, ProviderResult, binding_of, year_of};

const BASE: &str = "https://api2.isbndb.com";

async fn get(http: &reqwest::Client, key: &str, path: &str) -> ProviderResult<Option<Value>> {
    let res = http
        .get(format!("{BASE}{path}"))
        .header("authorization", key)
        .send()
        .await
        .map_err(|e| format!("ISBNdb: {e}"))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(format!("ISBNdb answered {}", res.status()));
    }
    res.json()
        .await
        .map(Some)
        .map_err(|e| format!("ISBNdb: {e}"))
}

fn candidate(b: &Value) -> Option<Candidate> {
    let isbn13 = b["isbn13"].as_str().and_then(mau_core::isbn::normalize);
    let id = isbn13
        .clone()
        .or_else(|| b["isbn"].as_str().map(String::from))?;
    let year = b["date_published"].as_str().and_then(year_of);
    let cover = b["image"].as_str().map(String::from);
    Some(Candidate {
        provider: "isbndb".into(),
        source_id: id.clone(),
        title: b["title"].as_str()?.to_string(),
        authors: b["authors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        first_year: year,
        cover_url: cover.clone(),
        edition_count: Some(1),
        editions: vec![CandidateEdition {
            source_id: Some(id),
            publisher: b["publisher"].as_str().map(String::from),
            binding: b["binding"].as_str().and_then(binding_of),
            year,
            isbn13,
            pages: b["pages"].as_i64().filter(|p| *p > 0),
            cover_url: cover,
        }],
    })
}

pub async fn search(http: &reqwest::Client, key: &str, q: &str) -> ProviderResult<Vec<Candidate>> {
    let path = format!("/books/{}?pageSize=20", urlencode(q));
    let Some(v) = get(http, key, &path).await? else {
        return Ok(vec![]);
    };
    Ok(v["books"]
        .as_array()
        .map(|b| b.iter().filter_map(candidate).collect())
        .unwrap_or_default())
}

pub async fn by_isbn(
    http: &reqwest::Client,
    key: &str,
    isbn: &str,
) -> ProviderResult<Option<Candidate>> {
    Ok(get(http, key, &format!("/book/{isbn}"))
        .await?
        .and_then(|v| candidate(&v["book"])))
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
