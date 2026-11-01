//! Hardcover (hardcover.app): needs an API token. Uses its GraphQL search.

use serde_json::{Value, json};

use super::{Candidate, CandidateEdition, ProviderResult};

const URL: &str = "https://api.hardcover.app/v1/graphql";
const QUERY: &str = "query Search($q: String!) { search(query: $q, query_type: \"Book\", per_page: 20) { results } }";

pub async fn search(http: &reqwest::Client, key: &str, q: &str) -> ProviderResult<Vec<Candidate>> {
    let token = if key.starts_with("Bearer ") {
        key.to_string()
    } else {
        format!("Bearer {key}")
    };
    let res = http
        .post(URL)
        .header("authorization", token)
        .json(&json!({ "query": QUERY, "variables": { "q": q } }))
        .send()
        .await
        .map_err(|e| format!("Hardcover: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Hardcover answered {}", res.status()));
    }
    let v: Value = res.json().await.map_err(|e| format!("Hardcover: {e}"))?;
    if let Some(err) = v["errors"][0]["message"].as_str() {
        return Err(format!("Hardcover: {err}"));
    }
    let hits = v["data"]["search"]["results"]["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(hits
        .iter()
        .filter_map(|h| candidate(&h["document"]))
        .collect())
}

fn candidate(d: &Value) -> Option<Candidate> {
    let id = match &d["id"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let isbn13 = d["isbns"].as_array().and_then(|a| {
        a.iter()
            .filter_map(|i| i.as_str())
            .find_map(mau_core::isbn::normalize)
    });
    let cover = d["image"]["url"].as_str().map(String::from);
    let year = d["release_year"].as_i64();
    Some(Candidate {
        provider: "hardcover".into(),
        source_id: id.clone(),
        title: d["title"].as_str()?.to_string(),
        authors: d["author_names"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        first_year: year,
        cover_url: cover.clone(),
        edition_count: d["editions_count"].as_i64(),
        editions: vec![CandidateEdition {
            source_id: Some(id),
            year,
            isbn13,
            pages: d["pages"].as_i64().filter(|p| *p > 0),
            cover_url: cover,
            ..Default::default()
        }],
    })
}

pub async fn by_isbn(
    http: &reqwest::Client,
    key: &str,
    isbn: &str,
) -> ProviderResult<Option<Candidate>> {
    Ok(search(http, key, isbn)
        .await?
        .into_iter()
        .find(|c| c.editions.iter().any(|e| e.isbn13.as_deref() == Some(isbn))))
}
