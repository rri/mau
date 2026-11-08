//! Parsing other services' exports into a common row format.
//!
//! Supported: Goodreads CSV, StoryGraph CSV, LibraryThing (TSV or JSON),
//! dinky.dev JSON, any CSV with a column mapping, and mau's own JSON export
//! (parsed by the server, since it carries full history).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One book from an import file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub title: String,
    pub authors: Vec<String>,
    pub isbn13: Option<String>,
    pub pages: Option<i64>,
    pub year: Option<i64>,
    pub publisher: Option<String>,
    /// 0 = none, else 1–5.
    pub rating: i64,
    /// Finished readings, oldest first. Each has an optional start and
    /// finish date (`YYYY-MM-DD`); a finished reading with no known date has
    /// `finished: None`.
    pub reads: Vec<Read>,
    pub tags: Vec<String>,
    pub owned: bool,
    /// Hardcover, Paperback, Ebook, or Audiobook, when the file says.
    pub binding: Option<String>,
    pub notes: String,
    /// When the book was added there (Unix ms), if the file says.
    pub added_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Read {
    pub started: Option<String>,
    pub finished: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("This doesn't look like a {0} export: {1}")]
    Format(&'static str, String),
}

pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Helpers

fn clean(s: &str) -> String {
    s.trim().to_string()
}

/// Goodreads wraps ISBNs as `="0441478123"`; LibraryThing as `[0441478123]`.
fn isbn_of(s: &str) -> Option<String> {
    let s = s
        .trim()
        .trim_start_matches('=')
        .trim_matches('"')
        .trim_matches(['[', ']']);
    s.split([',', ' ']).find_map(crate::isbn::normalize)
}

/// Accepts 2023/05/12, 2023-05-12, 2023-5-2, or a timestamp starting with a date.
pub fn date_of(s: &str) -> Option<String> {
    let s = s.trim();
    let parts: Vec<&str> = s
        .split(['/', '-', ' ', 'T'])
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    let (y, m, d) = (
        parts[0].parse::<u32>().ok()?,
        parts[1].parse::<u32>().ok()?,
        parts[2].parse::<u32>().ok()?,
    );
    ((1000..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d))
        .then(|| format!("{y:04}-{m:02}-{d:02}"))
}

fn int_of(s: &str) -> Option<i64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.round() as i64)
}

fn rating_of(s: &str) -> i64 {
    int_of(s).unwrap_or(0).clamp(0, 5)
}

fn binding_of(s: &str) -> Option<String> {
    let s = s.to_ascii_lowercase();
    let b = if s.contains("hard") {
        "Hardcover"
    } else if s.contains("paper") || s.contains("mass market") {
        "Paperback"
    } else if s.contains("ebook")
        || s.contains("kindle")
        || s.contains("digital")
        || s.contains("e-book")
    {
        "Ebook"
    } else if s.contains("audio") {
        "Audiobook"
    } else {
        return None;
    };
    Some(b.into())
}

fn names(s: &str) -> Vec<String> {
    s.split(',').map(clean).filter(|n| !n.is_empty()).collect()
}

fn joined_notes(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

type Record = HashMap<String, String>;

/// Reads a CSV/TSV into header → value maps.
fn records(text: &str, delimiter: u8) -> Result<(Vec<String>, Vec<Record>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.trim_start_matches('\u{feff}').as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| Error::Format("CSV", e.to_string()))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| Error::Format("CSV", e.to_string()))?;
        out.push(
            headers
                .iter()
                .cloned()
                .zip(rec.iter().map(String::from))
                .collect(),
        );
    }
    Ok((headers, out))
}

fn require(headers: &[String], cols: &[&str], what: &'static str) -> Result<()> {
    for c in cols {
        if !headers.iter().any(|h| h == c) {
            return Err(Error::Format(what, format!("missing the \"{c}\" column")));
        }
    }
    Ok(())
}

fn get<'a>(r: &'a HashMap<String, String>, k: &str) -> &'a str {
    r.get(k).map(String::as_str).unwrap_or("")
}

// ---------------------------------------------------------------------------
// Goodreads

pub fn goodreads(text: &str) -> Result<Vec<Row>> {
    let (headers, recs) = records(text, b',')?;
    require(
        &headers,
        &["Title", "Author", "Exclusive Shelf"],
        "Goodreads",
    )?;
    Ok(recs
        .iter()
        .map(|r| {
            let shelf = get(r, "Exclusive Shelf").trim();
            let mut authors = names(get(r, "Author"));
            authors.extend(names(get(r, "Additional Authors")));
            let read_count = int_of(get(r, "Read Count")).unwrap_or(1).max(1);
            let reads = if shelf == "read" {
                let finished = date_of(get(r, "Date Read"));
                // Goodreads only keeps the latest date; earlier re-reads are undated.
                let mut v: Vec<Read> = (1..read_count).map(|_| Read::default()).collect();
                v.push(Read {
                    started: None,
                    finished,
                });
                v
            } else {
                vec![]
            };
            Row {
                title: clean(get(r, "Title")),
                authors,
                isbn13: isbn_of(get(r, "ISBN13")).or_else(|| isbn_of(get(r, "ISBN"))),
                pages: int_of(get(r, "Number of Pages")).filter(|p| *p > 0),
                year: int_of(get(r, "Original Publication Year"))
                    .or_else(|| int_of(get(r, "Year Published"))),
                publisher: Some(clean(get(r, "Publisher"))).filter(|p| !p.is_empty()),
                rating: rating_of(get(r, "My Rating")),
                reads,
                tags: names(get(r, "Bookshelves"))
                    .into_iter()
                    .filter(|t| !["read", "currently-reading", "to-read"].contains(&t.as_str()))
                    .collect(),
                owned: int_of(get(r, "Owned Copies")).unwrap_or(0) > 0,
                binding: binding_of(get(r, "Binding")),
                notes: joined_notes(&[get(r, "My Review"), get(r, "Private Notes")]),
                added_at: None,
            }
        })
        .filter(|r| !r.title.is_empty())
        .collect())
}

// ---------------------------------------------------------------------------
// StoryGraph

/// "2023/01/05-2023/01/20, 2024/03/01" → reads.
fn storygraph_reads(dates: &str, last: &str) -> Vec<Read> {
    let mut reads: Vec<Read> = dates
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|range| {
            // Dates use '/', so a '-' separates start and finish.
            match range.split_once('-') {
                Some((a, b)) if date_of(a).is_some() && date_of(b).is_some() => Read {
                    started: date_of(a),
                    finished: date_of(b),
                },
                _ => Read {
                    started: None,
                    finished: date_of(range),
                },
            }
        })
        .collect();
    if reads.is_empty() {
        reads.push(Read {
            started: None,
            finished: date_of(last),
        });
    }
    reads.sort_by(|a, b| a.finished.cmp(&b.finished));
    reads
}

pub fn storygraph(text: &str) -> Result<Vec<Row>> {
    let (headers, recs) = records(text, b',')?;
    require(&headers, &["Title", "Authors", "Read Status"], "StoryGraph")?;
    Ok(recs
        .iter()
        .map(|r| {
            let finished = get(r, "Read Status").trim().eq_ignore_ascii_case("read");
            let owned = get(r, "Owned?").trim();
            Row {
                title: clean(get(r, "Title")),
                authors: names(get(r, "Authors")),
                isbn13: isbn_of(get(r, "ISBN/UID")),
                pages: None,
                year: None,
                publisher: None,
                rating: rating_of(get(r, "Star Rating")),
                reads: if finished {
                    storygraph_reads(get(r, "Dates Read"), get(r, "Last Date Read"))
                } else {
                    vec![]
                },
                tags: names(get(r, "Tags")),
                owned: owned.eq_ignore_ascii_case("yes") || owned.eq_ignore_ascii_case("true"),
                binding: binding_of(get(r, "Format")),
                notes: clean(get(r, "Review")),
                added_at: None,
            }
        })
        .filter(|r| !r.title.is_empty())
        .collect())
}

// ---------------------------------------------------------------------------
// LibraryThing

/// LibraryThing's "Your library" collection means owned; "Read but unowned",
/// "Wishlist", and "To read" don't.
fn lt_owned(collections: &[String]) -> bool {
    collections.is_empty()
        || collections
            .iter()
            .any(|c| c.eq_ignore_ascii_case("your library"))
}

pub fn librarything(text: &str) -> Result<Vec<Row>> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        return librarything_json(t);
    }
    let (headers, recs) = records(text, b'\t')?;
    require(&headers, &["Title", "Primary Author"], "LibraryThing")?;
    Ok(recs
        .iter()
        .map(|r| {
            let collections = names(get(r, "Collections"));
            let finished = date_of(get(r, "Date Read"));
            let in_read = collections
                .iter()
                .any(|c| c.eq_ignore_ascii_case("read but unowned"));
            Row {
                title: clean(get(r, "Title")),
                authors: {
                    let mut a = vec![clean(get(r, "Primary Author"))];
                    a.extend(names(get(r, "Secondary Author")));
                    a.retain(|n| !n.is_empty());
                    a
                },
                isbn13: isbn_of(get(r, "ISBNs")).or_else(|| isbn_of(get(r, "ISBN"))),
                pages: int_of(get(r, "Page Count")).filter(|p| *p > 0),
                year: int_of(get(r, "Date")).filter(|y| *y > 999),
                publisher: Some(clean(get(r, "Publication"))).filter(|p| !p.is_empty()),
                rating: rating_of(get(r, "Rating")),
                reads: if finished.is_some() || in_read {
                    vec![Read {
                        started: date_of(get(r, "Date Started")),
                        finished,
                    }]
                } else {
                    vec![]
                },
                tags: names(get(r, "Tags")),
                owned: lt_owned(&collections),
                binding: binding_of(get(r, "Media")),
                notes: joined_notes(&[
                    get(r, "Review"),
                    get(r, "Comment"),
                    get(r, "Private Comment"),
                ]),
                added_at: None,
            }
        })
        .filter(|r| !r.title.is_empty())
        .collect())
}

fn librarything_json(text: &str) -> Result<Vec<Row>> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::Format("LibraryThing JSON", e.to_string()))?;
    let books: Vec<&serde_json::Value> = match &v {
        serde_json::Value::Object(m) => m.values().collect(),
        serde_json::Value::Array(a) => a.iter().collect(),
        _ => vec![],
    };
    let s = |b: &serde_json::Value, k: &str| b[k].as_str().unwrap_or("").to_string();
    let list = |b: &serde_json::Value, k: &str| -> Vec<String> {
        b[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(books
        .into_iter()
        .map(|b| {
            let collections = list(b, "collections");
            let isbn = match &b["isbn"] {
                serde_json::Value::Object(m) => m
                    .values()
                    .filter_map(|x| x.as_str())
                    .find_map(crate::isbn::normalize),
                serde_json::Value::Array(a) => a
                    .iter()
                    .filter_map(|x| x.as_str())
                    .find_map(crate::isbn::normalize),
                serde_json::Value::String(x) => isbn_of(x),
                _ => None,
            };
            let finished = date_of(&s(b, "dateread"));
            let mut authors: Vec<String> = b["authors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x["fl"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if authors.is_empty() {
                authors = vec![s(b, "primaryauthor")];
            }
            authors.retain(|n| !n.trim().is_empty());
            Row {
                title: s(b, "title").trim().to_string(),
                authors,
                isbn13: isbn,
                pages: b["pages"].as_str().and_then(int_of).or(b["pages"].as_i64()),
                year: b["date"].as_str().and_then(int_of).filter(|y| *y > 999),
                publisher: None,
                rating: b["rating"]
                    .as_f64()
                    .map(|r| r.round() as i64)
                    .or_else(|| b["rating"].as_str().and_then(int_of))
                    .unwrap_or(0)
                    .clamp(0, 5),
                reads: if finished.is_some() {
                    vec![Read {
                        started: date_of(&s(b, "datestarted")),
                        finished,
                    }]
                } else {
                    vec![]
                },
                tags: list(b, "tags"),
                owned: lt_owned(&collections),
                binding: None,
                notes: joined_notes(&[&s(b, "review"), &s(b, "comment"), &s(b, "privatecomment")]),
                added_at: None,
            }
        })
        .filter(|r| !r.title.is_empty())
        .collect())
}

// ---------------------------------------------------------------------------
// dinky.dev

/// Unix ms for an ISO 8601 UTC timestamp such as `2022-06-05T03:24:57.708Z`.
fn timestamp_ms(s: &str) -> Option<i64> {
    let (date, time) = s.trim().strip_suffix('Z')?.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':');
    let (h, min) = (
        t.next()?.parse::<i64>().ok()?,
        t.next()?.parse::<i64>().ok()?,
    );
    let secs: f64 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || min > 59 || secs >= 61.0 {
        return None;
    }
    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + min) * 60_000 + (secs * 1000.0).round() as i64)
}

/// `[text](url)` becomes `text`.
fn strip_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else { break };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&after[..close]);
        rest = &after[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// A title without trailing parenthesized notes, for matching:
/// "Dune Messiah (Dune Series, #2)" → "Dune Messiah".
pub fn without_notes(title: &str) -> &str {
    let mut t = title.trim_end();
    while t.ends_with(')') {
        let mut depth = 0;
        let open = t.char_indices().rev().find_map(|(i, c)| {
            match c {
                ')' => depth += 1,
                '(' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        });
        match open {
            Some(i) if !t[..i].trim().is_empty() => t = t[..i].trim_end(),
            _ => break,
        }
    }
    t
}

/// dinky.dev's JSON export. Library items ("works") are lines of the form
/// `Title | Author; Author`. Deleted items are left out; archived ones (its
/// "done" list) are finished with an unknown date. Unfinished books come
/// first, newest first as dinky lists them, then finished ones.
pub fn dinky(text: &str) -> Result<Vec<Row>> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Error::Format("dinky.dev", e.to_string()))?;
    let works = v["contents"]["works"]
        .as_object()
        .ok_or_else(|| Error::Format("dinky.dev", "it has no library items".into()))?;
    let mut items: Vec<(bool, &str, Row)> = works
        .values()
        .filter(|w| w["deleted"].is_null())
        .filter_map(|w| {
            let line = strip_links(w["data"].as_str()?);
            let (title, authors) = line.split_once('|').unwrap_or((&line, ""));
            let archived = w["archive"].as_bool().unwrap_or(false);
            let row = Row {
                title: clean(title),
                authors: authors
                    .split(';')
                    .map(clean)
                    .filter(|a| !a.is_empty())
                    .collect(),
                reads: if archived {
                    vec![Read::default()]
                } else {
                    vec![]
                },
                added_at: w["created"].as_str().and_then(timestamp_ms),
                ..Default::default()
            };
            // dinky sorts unfinished items by created, done items by updated.
            let key = if archived {
                &w["updated"]
            } else {
                &w["created"]
            };
            Some((archived, key.as_str().unwrap_or(""), row))
        })
        .filter(|(_, _, r)| !r.title.is_empty())
        .collect();
    items.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(a.1))
            .then_with(|| a.2.title.cmp(&b.2.title))
    });
    Ok(items.into_iter().map(|(_, _, r)| r).collect())
}

// ---------------------------------------------------------------------------
// Any CSV, with a column mapping

/// The mau fields a CSV column can be mapped to.
pub const FIELDS: &[&str] = &[
    "title", "authors", "isbn", "state", "finished", "rating", "tags", "owned", "pages",
];

/// The header row and up to three sample rows, for the mapping screen.
pub fn csv_preview(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let delimiter = if text
        .lines()
        .next()
        .is_some_and(|l| l.contains('\t') && !l.contains(','))
    {
        b'\t'
    } else {
        b','
    };
    let (headers, recs) = records(text, delimiter)?;
    let samples = recs
        .iter()
        .take(3)
        .map(|r| headers.iter().map(|h| get(r, h).to_string()).collect())
        .collect();
    Ok((headers, samples))
}

/// Values that mean "finished" in a state column.
fn finished_word(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "read" | "finished" | "done" | "complete" | "completed" | "yes" | "true" | "1" | "x"
    )
}

fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1" | "x" | "owned" | "y"
    ) || int_of(s).is_some_and(|n| n > 0)
}

/// `mapping`: mau field → CSV column name.
pub fn generic(text: &str, mapping: &HashMap<String, String>) -> Result<Vec<Row>> {
    let has = |f: &str| mapping.get(f).is_some_and(|c| !c.is_empty());
    if !(has("isbn") || (has("title") && has("authors"))) {
        return Err(Error::Format(
            "mapped CSV",
            "map Title and Authors, or ISBN".into(),
        ));
    }
    let delimiter = if text
        .lines()
        .next()
        .is_some_and(|l| l.contains('\t') && !l.contains(','))
    {
        b'\t'
    } else {
        b','
    };
    let (_, recs) = records(text, delimiter)?;
    let col = |r: &HashMap<String, String>, f: &str| {
        mapping
            .get(f)
            .map(|c| get(r, c).to_string())
            .unwrap_or_default()
    };
    Ok(recs
        .iter()
        .map(|r| {
            let finished_date = date_of(&col(r, "finished"));
            let finished = finished_word(&col(r, "state")) || finished_date.is_some();
            Row {
                title: clean(&col(r, "title")),
                authors: names(&col(r, "authors")),
                isbn13: isbn_of(&col(r, "isbn")),
                pages: int_of(&col(r, "pages")).filter(|p| *p > 0),
                rating: rating_of(&col(r, "rating")),
                reads: if finished {
                    vec![Read {
                        started: None,
                        finished: finished_date,
                    }]
                } else {
                    vec![]
                },
                tags: names(&col(r, "tags")),
                owned: truthy(&col(r, "owned")),
                ..Default::default()
            }
        })
        .filter(|r| !r.title.is_empty() || r.isbn13.is_some())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOODREADS: &str = "Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,Average Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,Read Count,Owned Copies
1,Piranesi,Susanna Clarke,\"Clarke, Susanna\",,\"=\"\"1635575636\"\"\",\"=\"\"9781635575637\"\"\",4,4.2,Bloomsbury,Hardcover,272,2020,2020,2026/10/20,2026/09/28,\"favorites, read\",,read,Lovely.,,Re-read soon,1,1
2,Middlemarch,George Eliot,\"Eliot, George\",,\"=\"\"\"\"\",\"=\"\"\"\"\",0,4.0,,Paperback,880,1994,1871,,2026/08/01,currently-reading,,currently-reading,,,,0,0
3,Dune,Frank Herbert,\"Herbert, Frank\",,,,5,4.3,,,,,1965,,2020/01/01,read,,read,,,,2,0
";

    #[test]
    fn goodreads_rows() {
        let rows = goodreads(GOODREADS).unwrap();
        assert_eq!(rows.len(), 3);
        let p = &rows[0];
        assert_eq!(p.isbn13.as_deref(), Some("9781635575637"));
        assert_eq!(p.rating, 4);
        assert_eq!(
            p.reads,
            vec![Read {
                started: None,
                finished: Some("2026-10-20".into())
            }]
        );
        assert_eq!(p.tags, vec!["favorites"]);
        assert!(p.owned);
        assert_eq!(p.binding.as_deref(), Some("Hardcover"));
        assert_eq!(p.notes, "Lovely.\n\nRe-read soon");
        assert_eq!(p.year, Some(2020));
        // Currently reading: imported without a reading (inactive).
        assert!(rows[1].reads.is_empty());
        assert_eq!(rows[1].isbn13, None);
        // Read twice, no dates: two undated readings.
        assert_eq!(rows[2].reads.len(), 2);
        assert!(rows[2].reads.iter().all(|r| r.finished.is_none()));
    }

    #[test]
    fn goodreads_rejects_other_files() {
        assert!(goodreads("Title,Authors\nx,y\n").is_err());
    }

    #[test]
    fn storygraph_rows() {
        let csv = "Title,Authors,Contributors,ISBN/UID,Format,Read Status,Date Added,Last Date Read,Dates Read,Read Count,Star Rating,Review,Tags,Owned?
Kindred,Octavia E. Butler,,9780807083697,paperback,read,2025/12/01,2026/01/09,\"2025/12/02-2026/01/09, 2019/03/01-2019/03/20\",2,4.5,,\"book club, favorites\",Yes
Solaris,Stanisław Lem,,,digital,currently-reading,2026/02/01,,,0,,,,No
";
        let rows = storygraph(csv).unwrap();
        assert_eq!(rows[0].rating, 5);
        assert_eq!(rows[0].reads.len(), 2);
        assert_eq!(rows[0].reads[0].started.as_deref(), Some("2019-03-01"));
        assert_eq!(rows[0].reads[1].finished.as_deref(), Some("2026-01-09"));
        assert_eq!(rows[0].tags, vec!["book club", "favorites"]);
        assert!(rows[0].owned);
        assert!(rows[1].reads.is_empty());
        assert!(!rows[1].owned);
        assert_eq!(rows[1].binding.as_deref(), Some("Ebook"));
    }

    #[test]
    fn librarything_tsv_and_json() {
        let tsv = "Book Id\tTitle\tPrimary Author\tSecondary Author\tISBNs\tRating\tDate Read\tDate Started\tTags\tCollections\tPage Count\n\
                   1\tSolaris\tStanisław Lem\t\t[0156027607]\t4\t2025-12-01\t2025-11-01\tsci-fi, classics\tYour library\t204\n\
                   2\tHyperion\tDan Simmons\t\t\t0\t\t\t\tWishlist\t\n";
        let rows = librarything(tsv).unwrap();
        assert_eq!(rows[0].isbn13.as_deref(), Some("9780156027601"));
        assert_eq!(rows[0].reads[0].started.as_deref(), Some("2025-11-01"));
        assert!(rows[0].owned);
        assert!(!rows[1].owned);
        let json = r#"{"1": {"title": "Solaris", "primaryauthor": "Lem, Stanisław", "authors": [{"lf": "Lem, Stanisław", "fl": "Stanisław Lem"}], "isbn": {"0": "0156027607"}, "tags": ["sci-fi"], "collections": ["Your library"], "rating": 4, "dateread": "2025-12-01"}}"#;
        let rows = librarything(json).unwrap();
        assert_eq!(rows[0].authors, vec!["Stanisław Lem"]);
        assert_eq!(rows[0].reads.len(), 1);
    }

    #[test]
    fn dinky_rows() {
        let json = r#"{"settings": {}, "contents": {"tasks": {}, "works": {
            "a": {"data": "Old | Someone", "created": "2022-06-05T03:24:57.708Z", "updated": "2022-06-05T03:24:57.708Z"},
            "b": {"data": "Calculus | Ron Larson; Bruce H Edwards\u00a0", "created": "2023-05-07T20:59:01.352Z", "updated": "2023-05-07T20:59:01.352Z", "archive": false},
            "c": {"data": "Dune (Dune Series, #1) | Frank Herbert", "created": "2022-06-05T02:54:14.625Z", "updated": "2022-06-05T03:19:27.548Z", "archive": true, "today": "2023-01-01T00:00:00.000Z"},
            "d": {"data": "Gone | Nobody", "created": "2022-06-05T02:54:14.625Z", "updated": "2026-11-01T13:20:08.034Z", "deleted": "2026-11-01T13:20:08.034Z"},
            "e": {"data": " [The Implementation Of Functional Programming Languages](https://example.com/a.pdf) | Simon L Peyton Jones", "created": "2021-01-01T00:00:00.000Z", "updated": "2021-01-01T00:00:00.000Z"},
            "f": {"data": "Just a title", "created": "2020-01-01T00:00:00Z", "updated": "2020-01-01T00:00:00Z"}
        }}}"#;
        let rows = dinky(json).unwrap();
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Calculus",
                "Old",
                "The Implementation Of Functional Programming Languages",
                "Just a title",
                "Dune (Dune Series, #1)"
            ]
        );
        assert_eq!(rows[0].authors, ["Ron Larson", "Bruce H Edwards"]);
        assert!(rows[0].reads.is_empty());
        assert_eq!(rows[1].added_at, Some(1_654_399_497_708));
        assert!(rows[3].authors.is_empty());
        assert_eq!(rows[3].added_at, Some(1_577_836_800_000));
        assert_eq!(rows[4].reads, vec![Read::default()]);
        assert!(dinky("{}").is_err());
    }

    #[test]
    fn notes_come_off_titles() {
        assert_eq!(
            without_notes("Dune Messiah (Dune Series, #2)"),
            "Dune Messiah"
        );
        assert_eq!(
            without_notes("Facts And Mysteries (Revised Edition) (Particle Physics)"),
            "Facts And Mysteries"
        );
        assert_eq!(without_notes("(Untitled)"), "(Untitled)");
        assert_eq!(without_notes("Cosmos"), "Cosmos");
        assert_eq!(without_notes("A (b (c)) d"), "A (b (c)) d");
    }

    #[test]
    fn generic_mapping() {
        let csv = "Book Title,Writer(s),ISBN13,Status,Date Done,Stars,Labels,Have it\nDune,Frank Herbert,,done,2024-02-03,5,\"sci-fi, classics\",yes\nEmma,Jane Austen,,reading,,,,no\n";
        let map: HashMap<String, String> = [
            ("title", "Book Title"),
            ("authors", "Writer(s)"),
            ("isbn", "ISBN13"),
            ("state", "Status"),
            ("finished", "Date Done"),
            ("rating", "Stars"),
            ("tags", "Labels"),
            ("owned", "Have it"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let rows = generic(csv, &map).unwrap();
        assert_eq!(rows[0].reads[0].finished.as_deref(), Some("2024-02-03"));
        assert!(rows[0].owned);
        assert!(rows[1].reads.is_empty());
        let (headers, samples) = csv_preview(csv).unwrap();
        assert_eq!(headers[0], "Book Title");
        assert_eq!(samples.len(), 2);
        let bad: HashMap<String, String> = [("title".to_string(), "Book Title".to_string())].into();
        assert!(generic(csv, &bad).is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(date_of("2023/5/2").as_deref(), Some("2023-05-02"));
        assert_eq!(
            date_of("2023-05-12T10:00:00Z").as_deref(),
            Some("2023-05-12")
        );
        assert_eq!(date_of(""), None);
        assert_eq!(date_of("May 2023"), None);
    }
}
