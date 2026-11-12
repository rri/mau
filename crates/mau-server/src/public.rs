//! Public pages, rendered on the server so they work without JavaScript and
//! can be indexed: a user's profile (`/u/<username>`) listing everything they
//! publish, and each published item's own page (`/s/<token>`).

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::app::AppState;
use crate::error::ApiResult;

/// Books shown per section on the profile before "See all": two full rows
/// of five (phones, three across, show nine when there are more).
const PREVIEW: usize = 10;

struct Owner {
    id: String,
    username: String,
    display_name: String,
    bio: String,
}

struct ShareRow {
    kind: String,
    tag: Option<String>,
    include_ratings: bool,
    token: String,
}

struct Item {
    title: String,
    authors: String,
    cover: Option<String>,
    rating: i64,
    finished_on: Option<String>,
    pos: i64,
    total: i64,
    percent: bool,
}

fn month_year(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if date == crate::importer::UNKNOWN_DATE {
        return "Date unknown".into();
    }
    let m: usize = date.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!(
        "{} {}",
        MONTHS[m.clamp(1, 12) - 1],
        date.get(..4).unwrap_or("")
    )
}

fn title_of(s: &ShareRow) -> String {
    match s.kind.as_str() {
        "finished" => "Finished".into(),
        "active" => "Currently reading".into(),
        "catalog" => "Library".into(),
        _ => format!("#{}", s.tag.as_deref().unwrap_or("")),
    }
}

/// The books in a share, in display order.
async fn items(state: &AppState, owner: &str, s: &ShareRow) -> sqlx::Result<Vec<Item>> {
    // Each book with its latest reading.
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>, Option<String>, i64, Option<String>, Option<i64>, Option<i64>, i64, bool, String, Option<String>)> =
        sqlx::query_as(
            "SELECT w.title, b.title_override, b.authors_override,
                    COALESCE(b.cover_override, w.cover),
                    (SELECT group_concat(name, ', ') FROM (SELECT name FROM work_people p WHERE p.work_id = w.id AND p.role = 'author' ORDER BY position)),
                    b.rating, r.finished_on, r.pos, r.total, b.added_at, b.active, b.id, r.unit
             FROM books b JOIN works w ON w.id = b.work_id
             LEFT JOIN readings r ON r.id = (SELECT MAX(id) FROM readings x WHERE x.book_id = b.id AND x.deleted = 0)
             WHERE b.user_id = ? AND b.deleted = 0",
        )
        .bind(owner)
        .fetch_all(&state.db)
        .await?;
    let tagged: Vec<String> = match &s.tag {
        Some(t) => {
            sqlx::query_scalar(
                "SELECT book_id FROM book_tags WHERE user_id = ? AND tag = ? AND present = 1",
            )
            .bind(owner)
            .bind(t)
            .fetch_all(&state.db)
            .await?
        }
        None => vec![],
    };
    let mut list: Vec<(Item, bool, i64)> = rows
        .into_iter()
        .filter(|r| match s.kind.as_str() {
            "finished" => r.6.is_some(),
            "active" => r.10 && r.6.is_none(),
            "tag" => tagged.contains(&r.11),
            _ => true,
        })
        .map(|r| {
            let authors =
                r.2.and_then(|a| serde_json::from_str::<Vec<String>>(&a).ok())
                    .map(|a| a.join(", "))
                    .or(r.4)
                    .unwrap_or_default();
            (
                Item {
                    title: r.1.unwrap_or(r.0),
                    authors,
                    cover: r.3,
                    rating: if s.include_ratings { r.5 } else { 0 },
                    finished_on: r.6,
                    pos: r.7.unwrap_or(0),
                    total: r.8.unwrap_or(0),
                    percent: r.12.as_deref() == Some("percent"),
                },
                r.10,
                r.9,
            )
        })
        .collect();
    match s.kind.as_str() {
        "finished" => list.sort_by(|a, b| b.0.finished_on.cmp(&a.0.finished_on)),
        "active" => {}
        _ => list.sort_by_key(|x| std::cmp::Reverse(x.2)),
    }
    Ok(list.into_iter().map(|(i, _, _)| i).collect())
}

const STYLE: &str = r#"
:root{--bg:#f5f7f6;--surface:#fff;--ink:#0d1512;--muted:#56635d;--line:#e1e7e4;--accent:#047857;color-scheme:light}
@media (prefers-color-scheme:dark){:root{--bg:#0a0f0d;--surface:#131a17;--ink:#e9f0ec;--muted:#95a59e;--line:#25302b;--accent:#34d399;color-scheme:dark}}
*{box-sizing:border-box}html,body{margin:0}
body{background:var(--bg);color:var(--ink);font:15px/1.5 'Instrument Sans',system-ui,sans-serif;-webkit-font-smoothing:antialiased;-moz-osx-font-smoothing:grayscale}
main{max-width:720px;min-height:100vh;margin:0 auto;padding:36px 24px 48px;background:var(--surface);border-left:1px solid var(--line);border-right:1px solid var(--line)}
a{color:var(--accent)}
.head h1{margin:0;font-size:28px;font-weight:600;letter-spacing:-.015em;line-height:1.15}
.user{color:var(--muted)}
.bio{margin:10px 0 0;max-width:60ch}
section{margin-top:32px;padding-top:20px;border-top:1px solid var(--line)}
h2{margin:0 0 14px;font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);display:flex;justify-content:space-between;align-items:baseline}
h2 a{font-size:14px;letter-spacing:0;text-transform:none;font-weight:500}
.shelf{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:22px 16px;list-style:none;margin:0;padding:0}
.shelf.cut .bk:nth-child(n+10){display:none}
@media(min-width:560px){.shelf{grid-template-columns:repeat(5,minmax(0,1fr))}.shelf.cut .bk:nth-child(n+10){display:grid}}
.bk{display:grid;align-content:start;gap:6px;font-size:13.5px}
.cover{aspect-ratio:2/3;border-radius:5px;box-shadow:0 1px 2px rgb(0 0 0/.18),0 0 0 1px rgb(0 0 0/.06);overflow:hidden;display:grid;place-items:center;text-align:center;padding:8px;font-weight:700;font-size:12px;line-height:1.2;text-transform:uppercase;color:#f3f6f4}
.cover img{width:100%;height:100%;object-fit:cover;display:block}
.cover.img{padding:0}
.t{font-weight:600;line-height:1.3;letter-spacing:-.01em}
.d{color:var(--muted);font-size:13px;font-variant-numeric:tabular-nums}
.stars{color:var(--accent);letter-spacing:1px}
.reading{display:grid;grid-template-columns:52px minmax(0,1fr);gap:14px;align-items:center;margin-bottom:14px}
.bar{height:4px;border-radius:99px;background:var(--line);overflow:hidden;margin-top:7px}
.bar span{display:block;height:100%;border-radius:99px;background:var(--accent)}
.empty{color:var(--muted);padding:30px 0}
footer{margin-top:48px;font-size:13.5px;color:var(--muted)}
@media (max-width:760px){main{border:0}}
"#;

fn hue(s: &str) -> u32 {
    s.chars().fold(0u32, |h, c| {
        (h.wrapping_mul(31).wrapping_add(c as u32)) % 360
    })
}

fn cover(i: &Item) -> Markup {
    match &i.cover {
        Some(hash) => {
            html! { div.cover.img { img src=(format!("/covers/{hash}-m.jpg")) alt="" loading="lazy"; } }
        }
        None => {
            html! { div.cover style=(format!("background:hsl({} 42% 30%)", hue(&i.title))) { (i.title) } }
        }
    }
}

fn stars(n: i64) -> Markup {
    html! { @if n > 0 { " · " span.stars aria-label=(format!("{n} stars")) { ("★".repeat(n as usize)) } } }
}

/// Covers in full rows. `cut`: a profile preview with more to see, so a
/// part row is left out on phones.
fn shelf(list: &[Item], cut: bool) -> Markup {
    html! {
        ul.shelf.cut[cut] {
            @for i in list {
                li.bk {
                    (cover(i))
                    span.t { (i.title) }
                    span.d {
                        (i.authors)
                        @if let Some(f) = &i.finished_on { br; (month_year(f)) }
                        (stars(i.rating))
                    }
                }
            }
        }
    }
}

fn reading_list(list: &[Item]) -> Markup {
    html! {
        @for i in list {
            div.reading {
                (cover(i))
                div {
                    div.t { (i.title) }
                    @if i.percent {
                        div.d { (i.authors) " · " (i.pos) "%" }
                    } @else {
                        div.d { (i.authors) " · page " (i.pos) " of " (i.total) }
                    }
                    div.bar { span style=(format!("width:{}%", if i.total > 0 { 100 * i.pos / i.total } else { 0 })) {} }
                }
            }
        }
    }
}

/// One shared list. On the profile each list has a heading (and a link to
/// the whole list when it's cut short); on a share's own page the page title
/// already names it.
fn section(s: &ShareRow, list: &[Item], limit: Option<usize>, heading: bool) -> Markup {
    let shown = limit.map_or(list, |n| &list[..list.len().min(n)]);
    html! {
        section {
            @if heading {
                h2 {
                    span { (title_of(s)) }
                    @if shown.len() < list.len() {
                        a href=(format!("/s/{}", s.token)) { "See all " (list.len()) }
                    }
                }
            }
            @if list.is_empty() {
                p.empty { "Nothing here yet." }
            } @else if s.kind == "active" {
                (reading_list(shown))
            } @else {
                (shelf(shown, shown.len() < list.len()))
            }
        }
    }
}

fn page(title: &str, description: &str, body: Markup, public_url: &str) -> Markup {
    let font = crate::web::asset_path("instrument-sans-latin-wght-normal");
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                meta name="description" content=(description);
                meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)";
                meta name="theme-color" content="#131a17" media="(prefers-color-scheme: dark)";
                link rel="icon" href="/icon.svg" type="image/svg+xml";
                @if let Some(f) = &font {
                    link rel="preload" href=(f) as="font" type="font/woff2" crossorigin;
                    style { (PreEscaped(format!("@font-face{{font-family:'Instrument Sans';font-style:normal;font-weight:400 700;font-display:swap;src:url({f}) format('woff2')}}"))) }
                }
                style { (PreEscaped(STYLE)) }
            }
            body {
                main {
                    (body)
                    footer { "Shared from " a href=(format!("{public_url}/signin")) { "mau" } }
                }
            }
        }
    }
}

fn html_response(status: StatusCode, m: Markup) -> Response {
    let mut res = (status, m.into_string()).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    res
}

fn not_found(public_url: &str) -> Response {
    html_response(
        StatusCode::NOT_FOUND,
        page(
            "Not found · mau",
            "",
            html! { div.head { h1 { "Nothing here" } p.user { "This page doesn't exist, or is no longer shared." } } },
            public_url,
        ),
    )
}

async fn owner_by_name(state: &AppState, username: &str) -> sqlx::Result<Option<Owner>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, username, display_name, bio FROM users WHERE username = ? AND suspended = 0",
    )
    .bind(username.to_ascii_lowercase())
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(|(id, username, display_name, bio)| Owner {
        id,
        username,
        display_name,
        bio,
    }))
}

async fn shares_of(state: &AppState, user_id: &str) -> sqlx::Result<Vec<ShareRow>> {
    let rows: Vec<(String, Option<String>, bool, String)> =
        sqlx::query_as("SELECT kind, tag, include_ratings, token FROM shares WHERE user_id = ? ORDER BY created_at")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(kind, tag, include_ratings, token)| ShareRow {
            kind,
            tag,
            include_ratings,
            token,
        })
        .collect())
}

fn header(o: &Owner) -> Markup {
    html! {
        div.head {
            h1 { (o.display_name) }
            div.user { "@" (o.username) }
            @if !o.bio.is_empty() { p.bio { (o.bio) } }
        }
    }
}

/// `GET /u/<username>`
pub async fn profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> ApiResult<Response> {
    let url = &state.config.public_url;
    let Some(owner) = owner_by_name(&state, &username).await? else {
        return Ok(not_found(url));
    };
    let shares = shares_of(&state, &owner.id).await?;
    // Until something is published, the profile doesn't exist publicly, so
    // names and bios can't be looked up.
    if shares.is_empty() {
        return Ok(not_found(url));
    }
    let mut sections = Vec::new();
    for s in &shares {
        let list = items(&state, &owner.id, s).await?;
        sections.push(section(s, &list, Some(PREVIEW), true));
    }
    let body = html! {
        (header(&owner))
        @for s in sections { (s) }
    };
    let description = if owner.bio.is_empty() {
        format!("{}'s books on mau", owner.display_name)
    } else {
        owner.bio.clone()
    };
    Ok(html_response(
        StatusCode::OK,
        page(
            &format!("{} (@{}) · mau", owner.display_name, owner.username),
            &description,
            body,
            url,
        ),
    ))
}

/// `GET /s/<token>`
pub async fn share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Response> {
    let url = &state.config.public_url;
    let row: Option<(String, String, Option<String>, bool, String)> = sqlx::query_as(
        "SELECT user_id, kind, tag, include_ratings, token FROM shares WHERE token = ?",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?;
    let Some((user_id, kind, tag, include_ratings, token)) = row else {
        return Ok(not_found(url));
    };
    let owner: Option<(String, String, String)> = sqlx::query_as(
        "SELECT username, display_name, bio FROM users WHERE id = ? AND suspended = 0",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((username, display_name, bio)) = owner else {
        return Ok(not_found(url));
    };
    let s = ShareRow {
        kind,
        tag,
        include_ratings,
        token,
    };
    let list = items(&state, &user_id, &s).await?;
    let title = format!("{} · {display_name}", title_of(&s));
    let body = html! {
        div.head {
            h1 { (title_of(&s)) }
            div.user { a href=(format!("/u/{username}")) { (display_name) " (@" (username) ")" } }
        }
        (section(&s, &list, None, false))
    };
    let _ = bio;
    Ok(html_response(
        StatusCode::OK,
        page(
            &format!("{title} · mau"),
            &format!("{} books", list.len()),
            body,
            url,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use crate::app_tests::TestApp;
    use serde_json::json;

    #[tokio::test]
    async fn profile_lists_published_items_only() {
        let app = TestApp::new().await;
        let (_, cookie) = app.user("rri", false).await;
        sqlx::query("UPDATE users SET display_name = 'R. Reader', bio = 'Slow reader.'")
            .execute(&app.state.db)
            .await
            .unwrap();
        // Nothing published: the profile looks like an unknown name.
        let html = get_html(&app, "/u/rri").await;
        assert!(html.contains("Nothing here"));
        assert!(!html.contains("Slow reader."));
        // Unknown user.
        assert!(get_html(&app, "/u/nobody").await.contains("Nothing here"));

        // Publish finished books with ratings.
        let (status, share) = app
            .post(
                "/api/shares",
                json!({"kind": "finished", "include_ratings": true}),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, 200, "{share}");
        sqlx::query(
            "INSERT INTO works (id, title, created_at, seq) VALUES ('w', 'Piranesi', 0, 0)",
        )
        .execute(&app.state.db)
        .await
        .unwrap();
        let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'rri'")
            .fetch_one(&app.state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO books (id, user_id, work_id, rank, rating, added_at, seq) VALUES ('b', ?, 'w', 'V', 4, 0, 0)")
            .bind(&uid)
            .execute(&app.state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO readings (id, book_id, user_id, started_on, finished_on, total, pos, seq) VALUES ('r', 'b', ?, '2026-10-01', '2026-10-20', 245, 245, 0)")
            .bind(&uid)
            .execute(&app.state.db)
            .await
            .unwrap();
        let html = get_html(&app, "/u/rri").await;
        assert!(html.contains("Piranesi"));
        assert!(html.contains("Oct 2026"));
        assert!(html.contains("★★★★"));
        let page = get_html(&app, &format!("/s/{}", share["token"].as_str().unwrap())).await;
        assert!(page.contains("Piranesi"));
        assert!(!page.contains("Slow reader."));
    }

    #[tokio::test]
    async fn profile_previews_fill_whole_rows_and_show_percent() {
        let app = TestApp::new().await;
        let (uid, cookie) = app.user("rri", false).await;
        for kind in ["finished", "active"] {
            let (status, _) = app
                .post(
                    "/api/shares",
                    json!({ "kind": kind, "include_ratings": false }),
                    Some(&cookie),
                )
                .await;
            assert_eq!(status, 200);
        }
        let sql = |q: String| {
            let db = app.state.db.clone();
            async move {
                // Test data written here, nothing from outside.
                sqlx::query(sqlx::AssertSqlSafe(q))
                    .execute(&db)
                    .await
                    .unwrap();
            }
        };
        for i in 0..12 {
            sql(format!(
                "INSERT INTO works (id, title, created_at, seq) VALUES ('w{i}', 'Book {i}', 0, 0)"
            ))
            .await;
            sql(format!("INSERT INTO books (id, user_id, work_id, rank, added_at, seq) VALUES ('b{i}', '{uid}', 'w{i}', 'V', 0, 0)")).await;
            sql(format!("INSERT INTO readings (id, book_id, user_id, started_on, finished_on, total, pos, seq) VALUES ('r{i}', 'b{i}', '{uid}', '2026-10-01', '2026-10-20', 100, 100, 0)")).await;
        }
        sql(
            "INSERT INTO works (id, title, created_at, seq) VALUES ('wp', 'Percent Book', 0, 0)"
                .into(),
        )
        .await;
        sql(format!("INSERT INTO books (id, user_id, work_id, rank, active, added_at, seq) VALUES ('bp', '{uid}', 'wp', 'V', 1, 0, 0)")).await;
        sql(format!("INSERT INTO readings (id, book_id, user_id, started_on, unit, total, pos, seq) VALUES ('rp', 'bp', '{uid}', '2026-10-01', 'percent', 100, 42, 0)")).await;

        let html = get_html(&app, "/u/rri").await;
        // Ten covers (two rows of five), marked as cut short, and the link.
        assert_eq!(html.matches("<li class=\"bk\"").count(), 10, "{html}");
        assert!(html.contains("<ul class=\"shelf cut\""));
        assert!(html.contains("See all 12"));
        assert!(html.contains(" · 42%"));
        assert!(!html.contains("page 42 of 100"));
    }

    async fn get_html(app: &TestApp, path: &str) -> String {
        use axum::body::Body;
        use axum::http::Request;
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let res = crate::app::router(app.state.clone())
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }
}
