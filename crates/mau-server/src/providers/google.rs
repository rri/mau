//! Google Books: needs an API key. Results are volumes (editions); each is
//! offered as its own work with one edition.

use serde_json::Value;

use super::{Candidate, CandidateEdition, ProviderResult, https, year_of};

async fn volumes(http: &reqwest::Client, key: &str, q: &str) -> ProviderResult<Vec<Candidate>> {
    let res = http
        .get("https://www.googleapis.com/books/v1/volumes")
        .query(&[("q", q), ("maxResults", "20"), ("key", key)])
        .send()
        .await
        .map_err(|e| format!("Google Books: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Google Books answered {}", res.status()));
    }
    let v: Value = res.json().await.map_err(|e| format!("Google Books: {e}"))?;
    Ok(v["items"]
        .as_array()
        .map(|items| items.iter().filter_map(candidate).collect())
        .unwrap_or_default())
}

fn candidate(item: &Value) -> Option<Candidate> {
    let info = &item["volumeInfo"];
    let isbn13 = info["industryIdentifiers"]
        .as_array()
        .and_then(|ids| {
            ids.iter()
                .find(|i| i["type"] == "ISBN_13")
                .or_else(|| ids.iter().find(|i| i["type"] == "ISBN_10"))
        })
        .and_then(|i| i["identifier"].as_str())
        .and_then(mau_core::isbn::normalize);
    let cover = info["imageLinks"]["thumbnail"].as_str().map(https);
    let year = info["publishedDate"].as_str().and_then(year_of);
    Some(Candidate {
        provider: "google".into(),
        source_id: item["id"].as_str()?.to_string(),
        title: info["title"].as_str()?.to_string(),
        authors: info["authors"]
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
            source_id: item["id"].as_str().map(String::from),
            publisher: info["publisher"].as_str().map(String::from),
            // Google Books doesn't say whether a volume is a hardcover or paperback.
            binding: None,
            year,
            isbn13,
            pages: info["pageCount"].as_i64().filter(|p| *p > 0),
            cover_url: cover,
        }],
    })
}

pub async fn search(http: &reqwest::Client, key: &str, q: &str) -> ProviderResult<Vec<Candidate>> {
    volumes(http, key, q).await
}

pub async fn by_isbn(
    http: &reqwest::Client,
    key: &str,
    isbn: &str,
) -> ProviderResult<Option<Candidate>> {
    Ok(volumes(http, key, &format!("isbn:{isbn}"))
        .await?
        .into_iter()
        .next())
}
