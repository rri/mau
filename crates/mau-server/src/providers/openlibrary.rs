//! Open Library (openlibrary.org): free, no key. Works group editions.

use serde_json::Value;

use super::{Candidate, CandidateEdition, ProviderResult, binding_of, year_of};

const BASE: &str = "https://openlibrary.org";
const FIELDS: &str = "key,title,author_name,first_publish_year,cover_i,edition_count";

fn cover(id: &Value) -> Option<String> {
    id.as_i64()
        .filter(|&i| i > 0)
        .map(|i| format!("https://covers.openlibrary.org/b/id/{i}-L.jpg"))
}

async fn get(
    http: &reqwest::Client,
    path: &str,
    query: &[(&str, &str)],
) -> ProviderResult<Option<Value>> {
    let res = http
        .get(format!("{BASE}{path}"))
        .query(query)
        .send()
        .await
        .map_err(|e| format!("Open Library: {e}"))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(format!("Open Library answered {}", res.status()));
    }
    res.json()
        .await
        .map(Some)
        .map_err(|e| format!("Open Library: {e}"))
}

fn work_from_doc(doc: &Value) -> Option<Candidate> {
    let key = doc["key"]
        .as_str()?
        .trim_start_matches("/works/")
        .to_string();
    Some(Candidate {
        provider: "openlibrary".into(),
        source_id: key,
        title: doc["title"].as_str()?.to_string(),
        authors: doc["author_name"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        first_year: doc["first_publish_year"].as_i64(),
        cover_url: cover(&doc["cover_i"]),
        edition_count: doc["edition_count"].as_i64(),
        editions: vec![],
    })
}

pub async fn search(http: &reqwest::Client, q: &str) -> ProviderResult<Vec<Candidate>> {
    let Some(v) = get(
        http,
        "/search.json",
        &[("q", q), ("fields", FIELDS), ("limit", "20")],
    )
    .await?
    else {
        return Ok(vec![]);
    };
    Ok(v["docs"]
        .as_array()
        .map(|d| d.iter().filter_map(work_from_doc).collect())
        .unwrap_or_default())
}

fn edition(e: &Value) -> CandidateEdition {
    CandidateEdition {
        source_id: e["key"]
            .as_str()
            .map(|k| k.trim_start_matches("/books/").to_string()),
        publisher: e["publishers"][0].as_str().map(String::from),
        binding: e["physical_format"].as_str().and_then(binding_of),
        year: e["publish_date"].as_str().and_then(year_of),
        isbn13: e["isbn_13"][0]
            .as_str()
            .and_then(mau_core::isbn::normalize)
            .or_else(|| e["isbn_10"][0].as_str().and_then(mau_core::isbn::normalize)),
        pages: e["number_of_pages"].as_i64().filter(|p| *p > 0),
        cover_url: cover(&e["covers"][0]),
    }
}

pub async fn by_isbn(http: &reqwest::Client, isbn: &str) -> ProviderResult<Option<Candidate>> {
    let Some(ed) = get(http, &format!("/isbn/{isbn}.json"), &[]).await? else {
        return Ok(None);
    };
    let found = edition(&ed);
    let Some(work_key) = ed["works"][0]["key"].as_str() else {
        // An edition without a work: treat the edition as the work.
        return Ok(ed["title"].as_str().map(|t| Candidate {
            provider: "openlibrary".into(),
            source_id: found.source_id.clone().unwrap_or_default(),
            title: t.to_string(),
            authors: vec![],
            first_year: found.year,
            cover_url: found.cover_url.clone(),
            edition_count: Some(1),
            editions: vec![found],
        }));
    };
    // The search index has the first publication year and author names.
    let q = format!("key:{work_key}");
    let doc = get(
        http,
        "/search.json",
        &[("q", q.as_str()), ("fields", FIELDS), ("limit", "1")],
    )
    .await?
    .and_then(|v| v["docs"].get(0).cloned());
    let mut c = match doc.as_ref().and_then(work_from_doc) {
        Some(c) => c,
        None => Candidate {
            provider: "openlibrary".into(),
            source_id: work_key.trim_start_matches("/works/").to_string(),
            title: ed["title"].as_str().unwrap_or("Untitled").to_string(),
            authors: vec![],
            first_year: found.year,
            cover_url: found.cover_url.clone(),
            edition_count: None,
            editions: vec![],
        },
    };
    if c.cover_url.is_none() {
        c.cover_url = found.cover_url.clone();
    }
    c.editions = vec![found];
    Ok(Some(c))
}

/// A work by its ID (`OL…W`), as search shows it: title, authors, first year,
/// and the cover. The work record itself can lack a cover that search finds
/// through an edition.
pub async fn work(http: &reqwest::Client, work_id: &str) -> ProviderResult<Option<Candidate>> {
    let q = format!("key:/works/{work_id}");
    Ok(get(
        http,
        "/search.json",
        &[("q", q.as_str()), ("fields", FIELDS), ("limit", "1")],
    )
    .await?
    .and_then(|v| v["docs"].get(0).and_then(work_from_doc)))
}

pub async fn editions(
    http: &reqwest::Client,
    work_id: &str,
) -> ProviderResult<Vec<CandidateEdition>> {
    let Some(v) = get(
        http,
        &format!("/works/{work_id}/editions.json"),
        &[("limit", "50")],
    )
    .await?
    else {
        return Ok(vec![]);
    };
    Ok(v["entries"]
        .as_array()
        .map(|e| e.iter().map(edition).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Answers recorded from openlibrary.org on 2026-11-11 (testdata/README.md).
    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/src/providers/testdata/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn search_results_become_works() {
        let v = fixture("ol-search.json");
        let works: Vec<Candidate> = v["docs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(work_from_doc)
            .collect();
        assert_eq!(works.len(), v["docs"].as_array().unwrap().len());
        let w = &works[0];
        assert_eq!(w.provider, "openlibrary");
        assert_eq!(w.source_id, "OL59800W");
        assert_eq!(w.title, "The Left Hand of Darkness");
        assert_eq!(w.authors, ["Ursula K. Le Guin"]);
        assert_eq!(w.first_year, Some(1969));
        assert!(w.edition_count.unwrap() > 50);
        assert_eq!(
            w.cover_url.as_deref(),
            Some("https://covers.openlibrary.org/b/id/10618463-L.jpg")
        );
    }

    #[test]
    fn an_isbn_lookup_reads_the_edition() {
        let e = edition(&fixture("ol-isbn.json"));
        assert_eq!(e.source_id.as_deref(), Some("OL7525849M"));
        assert_eq!(e.publisher.as_deref(), Some("Ace"));
        assert_eq!(e.binding.as_deref(), Some("Paperback"));
        assert_eq!(e.pages, Some(304));
        assert_eq!(e.isbn13.as_deref(), Some("9780441478125"));
        assert_eq!(
            e.cover_url.as_deref(),
            Some("https://covers.openlibrary.org/b/id/284550-L.jpg")
        );
        assert!(e.year.is_some());
    }

    #[test]
    fn a_works_editions_are_listed_with_what_each_has() {
        let v = fixture("ol-editions.json");
        let list: Vec<CandidateEdition> = v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(edition)
            .collect();
        assert_eq!(list.len(), v["entries"].as_array().unwrap().len());
        assert!(
            list.iter()
                .all(|e| e.source_id.as_deref().is_some_and(|k| k.starts_with("OL")))
        );
        let first = &list[0];
        assert_eq!(first.publisher.as_deref(), Some("Mondadori"));
        assert_eq!(first.year, Some(2025));
        assert_eq!(first.isbn13.as_deref(), Some("9788835740988"));
    }

    #[test]
    fn missing_or_odd_fields_are_left_empty() {
        let e = edition(
            &serde_json::json!({"key": "/books/OL1M", "covers": [-1], "number_of_pages": 0}),
        );
        assert_eq!(e.cover_url, None);
        assert_eq!(e.pages, None);
        assert_eq!(e.isbn13, None);
        assert!(
            work_from_doc(&serde_json::json!({"key": "/works/OL1W"})).is_none(),
            "no title"
        );
    }
}
