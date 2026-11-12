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
        let status = res.status();
        return Err(refusal(status, res.json::<Value>().await.ok()));
    }
    let v: Value = res.json().await.map_err(|e| format!("Google Books: {e}"))?;
    Ok(v["items"]
        .as_array()
        .map(|items| items.iter().filter_map(candidate).collect())
        .unwrap_or_default())
}

/// Google explains itself, e.g. "API key not valid" or a used-up quota.
fn refusal(status: reqwest::StatusCode, body: Option<Value>) -> String {
    match body.as_ref().and_then(|v| v["error"]["message"].as_str()) {
        Some(m) => format!("Google Books refused: {m}"),
        None => format!("Google Books answered {status}"),
    }
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

/// A volume by its ID, or `None` if Google doesn't know it.
pub async fn volume(
    http: &reqwest::Client,
    key: &str,
    id: &str,
) -> ProviderResult<Option<Candidate>> {
    let res = http
        .get(format!("https://www.googleapis.com/books/v1/volumes/{id}"))
        .query(&[("key", key)])
        .send()
        .await
        .map_err(|e| format!("Google Books: {e}"))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        let status = res.status();
        return Err(refusal(status, res.json::<Value>().await.ok()));
    }
    let v: Value = res.json().await.map_err(|e| format!("Google Books: {e}"))?;
    Ok(candidate(&v))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/src/providers/testdata/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn refusals_carry_googles_reason() {
        // Recorded answers (2026-11-11): a used-up quota and a bad key.
        let quota = refusal(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            Some(fixture("gb-quota.json")),
        );
        assert!(
            quota.starts_with("Google Books refused: Quota exceeded"),
            "{quota}"
        );
        let key = refusal(
            reqwest::StatusCode::BAD_REQUEST,
            Some(fixture("gb-badkey.json")),
        );
        assert_eq!(
            key,
            "Google Books refused: API key not valid. Please pass a valid API key."
        );
        let none = refusal(reqwest::StatusCode::BAD_GATEWAY, None);
        assert_eq!(none, "Google Books answered 502 Bad Gateway");
    }

    #[test]
    fn a_volume_becomes_a_work_with_one_edition() {
        // Hand-made from Google's documented volume format (see testdata/README.md).
        let v = fixture("gb-volumes.json");
        let items: Vec<Candidate> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(candidate)
            .collect();
        assert_eq!(items.len(), 2, "the volume without a title is skipped");
        let c = &items[0];
        assert_eq!(c.provider, "google");
        assert_eq!(c.source_id, "lhodVol1");
        assert_eq!(c.title, "The Left Hand of Darkness");
        assert_eq!(c.authors, ["Ursula K. Le Guin"]);
        assert_eq!(c.first_year, Some(1987));
        assert_eq!(c.edition_count, Some(1));
        // Covers are fetched over HTTPS.
        assert!(
            c.cover_url
                .as_deref()
                .unwrap()
                .starts_with("https://books.google.com/")
        );
        let e = &c.editions[0];
        assert_eq!(e.isbn13.as_deref(), Some("9780441478125"));
        assert_eq!(e.publisher.as_deref(), Some("Ace"));
        assert_eq!(e.pages, Some(304));
        assert_eq!(e.binding, None);
        // Only an ISBN-10: converted to ISBN-13. No page count: none.
        let d = &items[1];
        assert_eq!(d.editions[0].isbn13.as_deref(), Some("9780441478125"));
        assert_eq!(d.editions[0].pages, None);
        assert!(d.authors.is_empty());
    }
}
