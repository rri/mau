//! Tokens, scopes, CORS, the public API, and the OAuth flow.

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::app_tests::{ORIGIN, TestApp};

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    text: String,
}

async fn call(
    app: &TestApp,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<String>,
) -> Res {
    let mut req = Request::builder().method(method).uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let req = req.body(body.map_or_else(Body::empty, Body::from)).unwrap();
    let res = crate::app::router(app.state.clone())
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    Res {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text,
    }
}

/// A request with a bearer token, as an app on another site would send it.
async fn api(app: &TestApp, method: &str, path: &str, token: &str, body: Option<Value>) -> Res {
    let auth = format!("Bearer {token}");
    let mut h = vec![
        ("authorization", auth.as_str()),
        ("origin", "https://stats.example.com"),
    ];
    if body.is_some() {
        h.push(("content-type", "application/json"));
    }
    call(app, method, path, &h, body.map(|b| b.to_string())).await
}

async fn form(app: &TestApp, path: &str, fields: &[(&str, &str)]) -> Res {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    call(
        app,
        "POST",
        path,
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "https://stats.example.com"),
        ],
        Some(body),
    )
    .await
}

/// Makes a personal token through Settings and returns its secret.
async fn pat(app: &TestApp, cookie: &str, scopes: &[&str]) -> String {
    let (status, body) = app
        .post(
            "/api/api-access/tokens",
            json!({"name": "script", "scopes": scopes, "expires_in_days": 30}),
            Some(cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["secret"].as_str().unwrap().to_string()
}

const ALL: &[&str] = &[
    "profile:read",
    "books:write",
    "books:delete",
    "tags:write",
    "reading:write",
    "ratings:write",
    "notes:write",
    "copies:write",
    "catalog:search",
    "shares:write",
];

#[tokio::test]
async fn personal_tokens_need_a_confirmation_and_are_shown_once() {
    let app = TestApp::new().await;
    let (id, _) = app.user("rri", false).await;
    let unconfirmed = crate::auth::session::create(&app.state.db, &id, "Other", false)
        .await
        .unwrap();
    let (status, body) = app
        .post(
            "/api/api-access/tokens",
            json!({"name": "x", "scopes": ["profile:read"], "expires_in_days": null}),
            Some(&unconfirmed),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "reauth_required");

    let (_, cookie) = app.user("juno", false).await;
    for bad in [
        json!({"name": "", "scopes": ["profile:read"], "expires_in_days": 30}),
        json!({"name": "x", "scopes": [], "expires_in_days": 30}),
        json!({"name": "x", "scopes": ["admin"], "expires_in_days": 30}),
        json!({"name": "x", "scopes": ["profile:read"], "expires_in_days": 7}),
    ] {
        let (status, _) = app
            .post("/api/api-access/tokens", bad.clone(), Some(&cookie))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (status, made) = app
        .post(
            "/api/api-access/tokens",
            json!({"name": "Backup sync", "scopes": ["profile:read"], "expires_in_days": null}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let secret = made["secret"].as_str().unwrap();
    assert!(secret.starts_with("mau_pat_"));
    assert_eq!(made["token"]["expires_at"], Value::Null);
    let (_, list) = app.get("/api/api-access", Some(&cookie)).await;
    assert_eq!(list["tokens"][0]["name"], "Backup sync");
    assert_eq!(list["tokens"][0]["hint"], &secret[..12]);
    assert!(
        !list.to_string().contains(secret),
        "the secret is never shown again"
    );
    // Only its hash is stored.
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM api_tokens")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert_ne!(stored, secret);

    let r = api(&app, "GET", "/api/v1/profile", secret, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["username"], "juno");
    assert!(r.body.get("is_admin").is_none());

    // Another user can't revoke it; its owner can.
    let token_id = list["tokens"][0]["id"].as_str().unwrap();
    let (_, other) = app.user("mallory", false).await;
    app.request(
        "DELETE",
        &format!("/api/api-access/tokens/{token_id}"),
        None,
        Some(&other),
    )
    .await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", secret, None)
            .await
            .status,
        StatusCode::OK
    );
    app.request(
        "DELETE",
        &format!("/api/api-access/tokens/{token_id}"),
        None,
        Some(&cookie),
    )
    .await;
    let r = api(&app, "GET", "/api/v1/profile", secret, None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["error"], "invalid_token");
    assert!(
        r.headers[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("invalid_token")
    );
}

#[tokio::test]
async fn scopes_are_checked_per_endpoint_and_per_field() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let writer = pat(&app, &cookie, &["books:write"]).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &writer,
        Some(json!({"manual": {"title": "Middlemarch", "authors": ["George Eliot"]}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let id = r.body["id"].as_str().unwrap().to_string();
    assert_eq!(r.body["state"], "inactive");
    assert_eq!(r.body["authors"], json!(["George Eliot"]));
    // Fields beyond books:read are left out.
    for f in ["notes", "rating", "copies", "reading"] {
        assert!(r.body.get(f).is_none(), "{f}");
    }
    // Notes need notes:write.
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{id}"),
        &writer,
        Some(json!({"notes": "x"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["error"], "insufficient_scope");
    assert!(
        r.headers[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("scope=\"notes:write\"")
    );
    // Deleting needs books:delete, not books:write.
    let r = api(
        &app,
        "DELETE",
        &format!("/api/v1/books/{id}"),
        &writer,
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let notes = pat(&app, &cookie, &["notes:write", "books:read"]).await;
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{id}"),
        &notes,
        Some(json!({"notes": "Slow going"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["notes"], "Slow going");
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{id}"),
        &notes,
        Some(json!({"rating": 5})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // The change reaches the reader's devices through sync.
    let (_, sync) = app
        .post("/api/sync", json!({"since": 0, "ops": []}), Some(&cookie))
        .await;
    assert_eq!(sync["changes"]["books"][0]["notes"], "Slow going");

    let reader = pat(&app, &cookie, &["books:read"]).await;
    let r = api(&app, "GET", "/api/v1/books", &reader, None).await;
    assert_eq!(r.body["books"][0]["title"], "Middlemarch");
    assert!(r.body["books"][0].get("notes").is_none());
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &reader, None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn books_are_only_their_owners() {
    let app = TestApp::new().await;
    let (_, rri) = app.user("rri", false).await;
    let (_, juno) = app.user("juno", false).await;
    let a = pat(&app, &rri, ALL).await;
    let b = pat(&app, &juno, ALL).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &a,
        Some(json!({"manual": {"title": "Mine", "authors": ["A"]}})),
    )
    .await;
    let id = r.body["id"].as_str().unwrap().to_string();
    let work = r.body["work_id"].as_str().unwrap().to_string();
    for (m, p, body) in [
        ("GET", format!("/api/v1/books/{id}"), None),
        (
            "PATCH",
            format!("/api/v1/books/{id}"),
            Some(json!({"notes": "mine now"})),
        ),
        ("DELETE", format!("/api/v1/books/{id}"), None),
        ("PUT", format!("/api/v1/books/{id}/tags/x"), None),
        (
            "POST",
            format!("/api/v1/books/{id}/copies"),
            Some(json!({"format": "Ebook"})),
        ),
        ("GET", format!("/api/v1/books/{id}/readings"), None),
        ("POST", format!("/api/v1/books/{id}/start"), None),
        ("POST", format!("/api/v1/books/{id}/finish"), None),
        (
            "POST",
            format!("/api/v1/books/{id}/progress"),
            Some(json!({"position": 3})),
        ),
    ] {
        let r = api(&app, m, &p, &b, body).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{m} {p}: {}", r.text);
    }
    // Someone else's private work can't be added either.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &b,
        Some(json!({"work_id": work})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text);
    let r = api(&app, "GET", "/api/v1/books", &b, None).await;
    assert_eq!(r.body["books"], json!([]));
}

#[tokio::test]
async fn reading_through_the_api_follows_the_apps_rules() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Dune", "authors": ["Frank Herbert"]}, "tags": ["sf"]})),
    )
    .await;
    let id = r.body["id"].as_str().unwrap().to_string();
    assert_eq!(r.body["tags"], json!(["sf"]));
    assert_eq!(r.body["reading"], Value::Null);

    // Progress needs a started reading; a start without a page count asks for one... or counts by percent.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"position": 5})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/start"),
        &t,
        Some(json!({"total": 400})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["state"], "active");
    assert_eq!(r.body["reading"]["position"], 0);
    assert_eq!(r.body["reading"]["total"], 400);

    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"position": 40, "date": "2026-11-10"})),
    )
    .await;
    assert_eq!(r.body["reading"]["position"], 40);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"add": 10})),
    )
    .await;
    assert_eq!(r.body["reading"]["position"], 50);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"position": 1, "add": 1})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"position": 1, "date": "2999-01-01"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "GET",
        &format!("/api/v1/books/{id}/readings"),
        &t,
        None,
    )
    .await;
    let events = &r.body["readings"][0]["events"];
    assert_eq!(events.as_array().unwrap().len(), 2);
    assert_eq!(
        events[0],
        json!({"date": "2026-11-10", "from": 0, "to": 40, "unit": "pages"})
    );

    let r = api(&app, "POST", &format!("/api/v1/books/{id}/pause"), &t, None).await;
    assert_eq!(r.body["state"], "inactive");
    let r = api(&app, "POST", &format!("/api/v1/books/{id}/start"), &t, None).await;
    assert_eq!(r.body["state"], "active");
    assert_eq!(r.body["reading"]["position"], 50, "resumed, not restarted");

    // Reaching the total finishes the book.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/progress"),
        &t,
        Some(json!({"position": 999})),
    )
    .await;
    assert_eq!(r.body["state"], "finished");
    assert_eq!(r.body["reading"]["position"], 400);
    let r = api(&app, "GET", "/api/v1/books?state=finished", &t, None).await;
    assert_eq!(r.body["books"].as_array().unwrap().len(), 1);
    let r = api(&app, "GET", "/api/v1/books?state=active", &t, None).await;
    assert_eq!(r.body["books"], json!([]));
    // Starting again is a re-read with the same page count.
    let r = api(&app, "POST", &format!("/api/v1/books/{id}/start"), &t, None).await;
    assert_eq!(r.body["reading"]["position"], 0);
    assert_eq!(r.body["reading"]["total"], 400);

    // A never-started book without a catalog page count finishes by percent.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Emma", "authors": ["Jane Austen"]}})),
    )
    .await;
    let emma = r.body["id"].as_str().unwrap().to_string();
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma}/finish"),
        &t,
        Some(json!({"date": "2026-11-01"})),
    )
    .await;
    assert_eq!(r.body["state"], "finished");
    assert_eq!(r.body["reading"]["unit"], "percent");
    assert_eq!(r.body["reading"]["finished_on"], "2026-11-01");

    // Copies, tags and deleting.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/copies"),
        &t,
        Some(json!({"format": "Paperback", "location": "Hall"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let copy = r.body["id"].as_str().unwrap().to_string();
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{id}/copies"),
        &t,
        Some(json!({"format": "Scroll"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/copies/{copy}"),
        &t,
        Some(json!({"location": "Study"})),
    )
    .await;
    assert_eq!(r.body["location"], "Study");
    api(
        &app,
        "DELETE",
        &format!("/api/v1/books/{id}/tags/sf"),
        &t,
        None,
    )
    .await;
    let r = api(&app, "GET", &format!("/api/v1/books/{id}"), &t, None).await;
    assert_eq!(r.body["tags"], json!([]));
    assert_eq!(r.body["copies"][0]["location"], "Study");
    assert_eq!(
        api(&app, "DELETE", &format!("/api/v1/books/{id}"), &t, None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        api(&app, "GET", &format!("/api/v1/books/{id}"), &t, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_lists_page_by_id() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    for i in 0..5 {
        api(
            &app,
            "POST",
            "/api/v1/books",
            &t,
            Some(json!({"manual": {"title": format!("B{i}"), "authors": ["A"]}})),
        )
        .await;
    }
    let r = api(&app, "GET", "/api/v1/books?limit=2", &t, None).await;
    assert_eq!(r.body["books"].as_array().unwrap().len(), 2);
    let mut seen = 2;
    let mut next = r.body["next"].as_str().unwrap().to_string();
    loop {
        let r = api(
            &app,
            "GET",
            &format!("/api/v1/books?limit=2&after={next}"),
            &t,
            None,
        )
        .await;
        seen += r.body["books"].as_array().unwrap().len();
        match r.body["next"].as_str() {
            Some(n) => next = n.to_string(),
            None => break,
        }
    }
    assert_eq!(seen, 5);
    assert_eq!(
        api(&app, "GET", "/api/v1/books?state=lost", &t, None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn tokens_and_cookies_never_cross() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("root", true).await;
    let t = pat(&app, &cookie, ALL).await;
    // A token reaches nothing outside /api/v1, even an admin's: not the
    // account, sessions, tokens, imports, invites or admin.
    for (m, p) in crate::access_tests::SIGNED_IN
        .iter()
        .chain(crate::access_tests::ADMIN)
    {
        let r = api(&app, m, p, &t, Some(json!({}))).await;
        assert!(
            r.status == StatusCode::UNAUTHORIZED || r.status == StatusCode::FORBIDDEN,
            "{m} {p}: {}",
            r.status
        );
    }
    // The cookie doesn't work on /api/v1.
    let c = format!("mau_session={cookie}");
    let r = call(
        &app,
        "GET",
        "/api/v1/profile",
        &[("cookie", &c), ("origin", ORIGIN)],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // A refresh token isn't an access token.
    let r = api(&app, "GET", "/api/v1/profile", "mau_rt_abc", None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = api(&app, "GET", "/api/v1/profile", "Basic xyz", None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cors_is_open_for_tokens_and_closed_for_cookies() {
    let app = TestApp::new().await;
    let preflight = |path: &'static str| {
        let app = &app;
        async move {
            call(
                app,
                "OPTIONS",
                path,
                &[
                    ("origin", "https://stats.example.com"),
                    ("access-control-request-method", "POST"),
                    (
                        "access-control-request-headers",
                        "authorization, content-type",
                    ),
                ],
                None,
            )
            .await
        }
    };
    for path in ["/api/v1/books", "/oauth/token", "/oauth/revoke"] {
        let r = preflight(path).await;
        assert!(r.status.is_success(), "{path}: {}", r.status);
        assert_eq!(r.headers["access-control-allow-origin"], "*", "{path}");
        assert!(r.headers.get("access-control-allow-credentials").is_none());
        let allowed = r.headers["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .to_ascii_lowercase();
        assert!(allowed.contains("authorization"));
    }
    // Errors carry the headers too, so an app can read them.
    let r = api(&app, "GET", "/api/v1/profile", "mau_pat_nope", None).await;
    assert_eq!(r.headers["access-control-allow-origin"], "*");
    assert!(
        r.headers["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("www-authenticate")
    );
    let r = call(
        &app,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[("origin", "https://x.example")],
        None,
    )
    .await;
    assert_eq!(r.headers["access-control-allow-origin"], "*");
    assert_eq!(r.body["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(r.body["issuer"], "http://localhost:8370");

    // The cookie API sends no CORS headers and still checks the origin.
    let r = preflight("/api/sync").await;
    assert!(r.headers.get("access-control-allow-origin").is_none());
    let r = call(
        &app,
        "GET",
        "/api/auth/status",
        &[("origin", "https://stats.example.com")],
        None,
    )
    .await;
    assert!(r.headers.get("access-control-allow-origin").is_none());
    let r = call(
        &app,
        "POST",
        "/api/auth/logout",
        &[("origin", "https://stats.example.com")],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = call(
        &app,
        "GET",
        "/oauth/authorize",
        &[("origin", "https://stats.example.com")],
        None,
    )
    .await;
    assert!(r.headers.get("access-control-allow-origin").is_none());
}

// ---------------------------------------------------------------------------
// OAuth

const APP: &str = "https://stats.example.com/";
const CB: &str = "https://stats.example.com/callback.html";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk_and_more";

fn challenge(v: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes()))
}

fn authorize_url(scope: &str, extra: &[(&str, &str)]) -> String {
    let ch = challenge(VERIFIER);
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.extend_pairs([
        ("response_type", "code"),
        ("client_id", APP),
        ("redirect_uri", CB),
        ("scope", scope),
        ("state", "xyz"),
        ("code_challenge", ch.as_str()),
        ("code_challenge_method", "S256"),
    ]);
    q.extend_pairs(extra);
    format!("/oauth/authorize?{}", q.finish())
}

fn location(r: &Res) -> String {
    r.headers[header::LOCATION].to_str().unwrap().to_string()
}

fn param(url: &str, name: &str) -> Option<String> {
    let u = url::Url::parse(url).unwrap();
    let mut pairs: Vec<(String, String)> = u.query_pairs().into_owned().collect();
    if let Some(f) = u.fragment() {
        pairs.extend(url::form_urlencoded::parse(f.as_bytes()).into_owned());
    }
    pairs.into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

/// Starts a request and returns its ID (from the redirect to the web app).
async fn start_request(app: &TestApp, scope: &str) -> String {
    let r = call(app, "GET", &authorize_url(scope, &[]), &[], None).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text);
    location(&r)
        .strip_prefix("/authorize/")
        .unwrap()
        .to_string()
}

/// Signs in to the app through the whole flow; returns the token answer.
async fn connect(app: &TestApp, cookie: &str, scope: &str) -> Value {
    let id = start_request(app, scope).await;
    let (status, ans) = app
        .post(
            &format!("/api/oauth/requests/{id}/approve"),
            json!({}),
            Some(cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ans}");
    let redirect = ans["redirect"].as_str().unwrap();
    assert!(redirect.starts_with(CB));
    assert_eq!(param(redirect, "state").as_deref(), Some("xyz"));
    assert_eq!(
        param(redirect, "iss").as_deref(),
        Some("http://localhost:8370")
    );
    let code = param(redirect, "code").unwrap();
    let r = form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", CB),
            ("client_id", APP),
            ("code_verifier", VERIFIER),
        ],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    r.body
}

#[tokio::test]
async fn an_app_signs_in_and_gets_exactly_what_it_asked_for() {
    let app = TestApp::new().await;
    let (id, cookie) = app.user("rri", false).await;
    let req = start_request(&app, "profile:read books:read").await;
    let (status, info) = app
        .get(&format!("/api/oauth/requests/{req}"), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["host"], "stats.example.com");
    assert_eq!(info["redirect_to"], "stats.example.com");
    assert_eq!(info["me"]["username"], "rri");
    assert_eq!(info["scopes"].as_array().unwrap().len(), 2);

    // Allowing needs a recent password confirmation; the request survives.
    let fresh = crate::auth::session::create(&app.state.db, &id, "Other", false)
        .await
        .unwrap();
    let (status, body) = app
        .post(
            &format!("/api/oauth/requests/{req}/approve"),
            json!({}),
            Some(&fresh),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "reauth_required");
    let (status, _) = app
        .get(&format!("/api/oauth/requests/{req}"), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK);

    let tokens = connect(&app, &cookie, "profile:read books:read").await;
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    assert_eq!(tokens["scope"], "profile:read books:read");
    let access = tokens["access_token"].as_str().unwrap();
    assert!(access.starts_with("mau_at_"));
    let r = api(&app, "GET", "/api/v1/profile", access, None).await;
    assert_eq!(r.body["username"], "rri");
    let r = api(&app, "GET", "/api/v1/books/x/readings", access, None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Connected apps lists it; revoking it ends its tokens.
    let (_, list) = app.get("/api/api-access", Some(&cookie)).await;
    assert_eq!(list["apps"][0]["host"], "stats.example.com");
    let grant = list["apps"][0]["id"].as_str().unwrap();
    app.request(
        "DELETE",
        &format!("/api/api-access/apps/{grant}"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", access, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn codes_and_refresh_tokens_work_once() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;

    // A code: wrong verifier, wrong redirect, used twice.
    let req = start_request(&app, "profile:read").await;
    let (_, ans) = app
        .post(
            &format!("/api/oauth/requests/{req}/approve"),
            json!({}),
            Some(&cookie),
        )
        .await;
    let code = param(ans["redirect"].as_str().unwrap(), "code").unwrap();
    let exchange = |verifier: &'static str, redirect: &'static str| {
        let app = &app;
        let code = code.clone();
        async move {
            form(
                app,
                "/oauth/token",
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", redirect),
                    ("client_id", APP),
                    ("code_verifier", verifier),
                ],
            )
            .await
        }
    };
    let r = exchange("wrong-verifier-wrong-verifier-wrong-verifier-x", CB).await;
    assert_eq!(r.body["error"], "invalid_grant");
    let r = exchange(VERIFIER, CB).await;
    assert_eq!(
        r.body["error"], "invalid_grant",
        "a code that failed once is spent"
    );

    let tokens = connect(&app, &cookie, "profile:read").await;
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let refresh = tokens["refresh_token"].as_str().unwrap().to_string();
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", "https://other.example/"),
        ],
    )
    .await;
    assert_eq!(
        r.body["error"], "invalid_grant",
        "only its own app may use it"
    );
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", APP),
            ("scope", "notes:read"),
        ],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_scope");
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", APP),
        ],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let access2 = r.body["access_token"].as_str().unwrap().to_string();
    assert_ne!(r.body["refresh_token"], refresh);
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &access2, None)
            .await
            .status,
        StatusCode::OK
    );

    // The old refresh token again: copied, so the app's access ends.
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", APP),
        ],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_grant");
    for t in [&access, &access2] {
        assert_eq!(
            api(&app, "GET", "/api/v1/profile", t, None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let (_, list) = app.get("/api/api-access", Some(&cookie)).await;
    assert_eq!(list["apps"], json!([]));

    // A reused code ends the access it gave.
    let req = start_request(&app, "profile:read").await;
    let (_, ans) = app
        .post(
            &format!("/api/oauth/requests/{req}/approve"),
            json!({}),
            Some(&cookie),
        )
        .await;
    let code = param(ans["redirect"].as_str().unwrap(), "code").unwrap();
    let fields = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", CB),
        ("client_id", APP),
        ("code_verifier", VERIFIER),
    ];
    let r = form(&app, "/oauth/token", &fields).await;
    let access = r.body["access_token"].as_str().unwrap().to_string();
    let r = form(&app, "/oauth/token", &fields).await;
    assert_eq!(r.body["error"], "invalid_grant");
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &access, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    // Secrets aren't a thing here; unknown grants are refused.
    let r = form(
        &app,
        "/oauth/token",
        &[("grant_type", "client_credentials"), ("client_id", APP)],
    )
    .await;
    assert_eq!(r.body["error"], "unsupported_grant_type");
}

#[tokio::test]
async fn bad_links_get_mau_s_own_error_page_never_a_redirect() {
    let app = TestApp::new().await;
    for (client, redirect) in [
        ("", CB),
        ("http://stats.example.com/", CB),
        ("https://10.0.0.1/", "https://10.0.0.1/cb"),
        (APP, "https://evil.example/cb"),
        (APP, "https://stats.example.com.evil.example/cb"),
        (APP, "http://127.0.0.1:9000/cb"),
        (APP, "com.example.stats:/cb"),
        (APP, ""),
    ] {
        let mut q = url::form_urlencoded::Serializer::new(String::new());
        q.extend_pairs([
            ("response_type", "code"),
            ("client_id", client),
            ("redirect_uri", redirect),
            ("scope", "profile:read"),
            ("state", "s"),
            ("code_challenge", &challenge(VERIFIER)),
            ("code_challenge_method", "S256"),
        ]);
        let r = call(
            &app,
            "GET",
            &format!("/oauth/authorize?{}", q.finish()),
            &[],
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{client} {redirect}");
        assert!(r.headers.get(header::LOCATION).is_none());
        assert!(r.text.contains("This sign-in link doesn"), "{}", r.text);
    }
    // Problems the app can be told about go back to it.
    for (extra, error) in [
        (vec![("scope", "admin")], "invalid_scope"),
        (vec![("scope", "")], "invalid_scope"),
        (vec![("code_challenge_method", "plain")], "invalid_request"),
        (vec![("code_challenge", "short")], "invalid_request"),
        (vec![("state", "")], "invalid_request"),
        (
            vec![("response_type", "token")],
            "unsupported_response_type",
        ),
    ] {
        // Later pairs replace earlier ones: build the query by hand.
        let ch = challenge(VERIFIER);
        let mut pairs: Vec<(&str, &str)> = vec![
            ("response_type", "code"),
            ("client_id", APP),
            ("redirect_uri", CB),
            ("scope", "profile:read"),
            ("state", "s"),
            ("code_challenge", &ch),
            ("code_challenge_method", "S256"),
        ];
        for (k, v) in &extra {
            pairs.retain(|(pk, _)| pk != k);
            pairs.push((k, v));
        }
        let q = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        let r = call(&app, "GET", &format!("/oauth/authorize?{q}"), &[], None).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{extra:?}");
        let to = location(&r);
        assert!(to.starts_with(CB), "{to}");
        assert_eq!(param(&to, "error").as_deref(), Some(error), "{extra:?}");
        assert_eq!(param(&to, "iss").as_deref(), Some("http://localhost:8370"));
    }
}

#[tokio::test]
async fn native_apps_redirect_off_their_origin_only_when_their_metadata_says() {
    let app = TestApp::new().await;
    let loopback = "http://127.0.0.1:53682/cb";
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    let ch = challenge(VERIFIER);
    q.extend_pairs([
        ("response_type", "code"),
        ("client_id", APP),
        ("redirect_uri", loopback),
        ("scope", "profile:read"),
        ("state", "s"),
        ("code_challenge", ch.as_str()),
        ("code_challenge_method", "S256"),
        ("response_mode", "fragment"),
    ]);
    let path = format!("/oauth/authorize?{}", q.finish());
    assert_eq!(
        call(&app, "GET", &path, &[], None).await.status,
        StatusCode::BAD_REQUEST
    );
    app.state.oauth_meta.set(
        APP,
        Some(super::client::Metadata {
            name: Some("Reading Stats".into()),
            redirect_uris: vec!["http://127.0.0.1/cb".into()],
        }),
    );
    let r = call(&app, "GET", &path, &[], None).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text);
    let req = location(&r)
        .strip_prefix("/authorize/")
        .unwrap()
        .to_string();
    let (_, cookie) = app.user("rri", false).await;
    let (_, info) = app
        .get(&format!("/api/oauth/requests/{req}"), Some(&cookie))
        .await;
    assert_eq!(info["name"], "Reading Stats");
    assert_eq!(info["host"], "stats.example.com");
    assert_eq!(info["redirect_to"], "127.0.0.1:53682");
    let (_, ans) = app
        .post(
            &format!("/api/oauth/requests/{req}/approve"),
            json!({}),
            Some(&cookie),
        )
        .await;
    let redirect = ans["redirect"].as_str().unwrap();
    assert!(
        redirect.starts_with("http://127.0.0.1:53682/cb#"),
        "the code is in the fragment: {redirect}"
    );
    assert!(param(redirect, "code").is_some());
}

#[tokio::test]
async fn denying_tells_the_app() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let req = start_request(&app, "profile:read").await;
    let (_, ans) = app
        .post(
            &format!("/api/oauth/requests/{req}/deny"),
            json!({}),
            Some(&cookie),
        )
        .await;
    let to = ans["redirect"].as_str().unwrap();
    assert_eq!(param(to, "error").as_deref(), Some("access_denied"));
    assert_eq!(param(to, "state").as_deref(), Some("xyz"));
    let (status, _) = app
        .post(
            &format!("/api/oauth/requests/{req}/approve"),
            json!({}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an answered request is gone");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_clients")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 0, "links nobody accepted leave nothing behind");
}

#[tokio::test]
async fn admins_see_apps_and_can_block_them() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("root", true).await;
    let (_, cookie) = app.user("rri", false).await;
    let tokens = connect(&app, &cookie, "profile:read").await;
    let access = tokens["access_token"].as_str().unwrap();
    let (status, _) = app.get("/api/admin/apps", Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, apps) = app.get("/api/admin/apps", Some(&admin)).await;
    assert_eq!(apps[0]["host"], "stats.example.com");
    assert_eq!(apps[0]["people"], 1);
    assert!(
        !apps.to_string().contains("rri"),
        "admins see how many, not who"
    );

    let (status, _) = app
        .post(
            "/api/admin/apps/block",
            json!({"client_id": APP, "blocked": true}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", access, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let r = call(&app, "GET", &authorize_url("profile:read", &[]), &[], None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r.text.contains("blocked"));
    let (_, apps) = app.get("/api/admin/apps", Some(&admin)).await;
    assert_eq!(apps[0]["blocked"], true);
    assert_eq!(apps[0]["people"], 0);
    let (_, log) = app.get("/api/admin/audit", Some(&admin)).await;
    assert!(
        log.to_string()
            .contains("blocked the app https://stats.example.com")
    );

    app.post(
        "/api/admin/apps/block",
        json!({"client_id": APP, "blocked": false}),
        Some(&admin),
    )
    .await;
    connect(&app, &cookie, "profile:read").await;
}

#[tokio::test]
async fn recovering_an_account_ends_its_tokens_and_suspension_pauses_them() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("root", true).await;
    let (id, cookie) = app.user("juno", false).await;
    let personal = pat(&app, &cookie, &["profile:read"]).await;
    let oauth = connect(&app, &cookie, "profile:read").await;
    let access = oauth["access_token"].as_str().unwrap().to_string();

    // Suspended: refused, and back when unsuspended.
    app.request(
        "PATCH",
        &format!("/api/admin/users/{id}"),
        Some(json!({"suspended": true})),
        Some(&admin),
    )
    .await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &personal, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", oauth["refresh_token"].as_str().unwrap()),
            ("client_id", APP),
        ],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_grant");
    app.request(
        "PATCH",
        &format!("/api/admin/users/{id}"),
        Some(json!({"suspended": false})),
        Some(&admin),
    )
    .await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &personal, None)
            .await
            .status,
        StatusCode::OK
    );

    // A reset link ends everything.
    let (_, link) = app
        .post(
            &format!("/api/admin/users/{id}/reset-link"),
            json!({}),
            Some(&admin),
        )
        .await;
    let token = link["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let (status, _) = app
        .post(
            &format!("/api/auth/reset/{token}"),
            json!({"password": "juno's new password"}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    for t in [&personal, &access] {
        assert_eq!(
            api(&app, "GET", "/api/v1/profile", t, None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn revoking_through_the_endpoint() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let tokens = connect(&app, &cookie, "profile:read").await;
    let access = tokens["access_token"].as_str().unwrap();
    let refresh = tokens["refresh_token"].as_str().unwrap();
    // Another app can't revoke it; unknown tokens still answer 200.
    let r = form(
        &app,
        "/oauth/revoke",
        &[("token", refresh), ("client_id", "https://other.example/")],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", access, None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        form(&app, "/oauth/revoke", &[("token", "nope")])
            .await
            .status,
        StatusCode::OK
    );
    form(
        &app,
        "/oauth/revoke",
        &[("token", refresh), ("client_id", APP)],
    )
    .await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", access, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // A leaked personal token can be killed by whoever finds it.
    let p = pat(&app, &cookie, &["profile:read"]).await;
    form(&app, "/oauth/revoke", &[("token", &p)]).await;
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", &p, None).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn an_app_refreshing_often_doesnt_pile_up_tokens() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let mut refresh = connect(&app, &cookie, "profile:read").await["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();
    // 55 refreshes: under the endpoint's 60 a minute, over the caps.
    for _ in 0..55 {
        let r = form(
            &app,
            "/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh),
                ("client_id", APP),
            ],
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text);
        refresh = r.body["refresh_token"].as_str().unwrap().to_string();
    }
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT kind || CASE WHEN used_at IS NULL THEN '' ELSE '-spent' END, COUNT(*) FROM api_tokens GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&app.state.db)
    .await
    .unwrap();
    assert_eq!(
        counts,
        vec![
            ("access".to_string(), 20),
            ("refresh".to_string(), 1),
            ("refresh-spent".to_string(), 50)
        ]
    );
    // The latest refresh token still works.
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", APP),
        ],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn dates_from_apps_are_checked() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Emma", "authors": ["Jane Austen"]}})),
    )
    .await;
    let id = r.body["id"].as_str().unwrap().to_string();
    for bad in [
        "0000-01-01",
        "1850-01-01",
        "2026-13-01",
        "2026-1-1",
        "tomorrow",
    ] {
        let r = api(
            &app,
            "POST",
            &format!("/api/v1/books/{id}/finish"),
            &t,
            Some(json!({"date": bad})),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn admins_dont_learn_who_connected_what() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("root", true).await;
    let (_, cookie) = app.user("rri", false).await;
    connect(&app, &cookie, "profile:read").await;
    pat(&app, &cookie, &["profile:read"]).await;
    let (_, log) = app.get("/api/admin/audit", Some(&admin)).await;
    let log = log.to_string();
    assert!(
        !log.contains("stats.example.com") && !log.contains("token"),
        "{log}"
    );
}

// ---------------------------------------------------------------------------
// Coverage: every endpoint's scope, and each endpoint's own behavior

/// A reader with one book (with a copy) and one share; returns the cookie,
/// an all-scopes token, the book and the copy.
async fn library_with_one_book(app: &TestApp) -> (String, String, String, String) {
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(app, &cookie, ALL).await;
    let r = api(
        app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Dune", "authors": ["Frank Herbert"]}})),
    )
    .await;
    let book = r.body["id"].as_str().unwrap().to_string();
    let r = api(
        app,
        "POST",
        &format!("/api/v1/books/{book}/copies"),
        &t,
        Some(json!({"format": "Paperback"})),
    )
    .await;
    let copy = r.body["id"].as_str().unwrap().to_string();
    (cookie, t, book, copy)
}

#[tokio::test]
async fn every_endpoint_refuses_a_token_without_its_scope() {
    let app = TestApp::new().await;
    let (cookie, t, book, copy) = library_with_one_book(&app).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/shares",
        &t,
        Some(json!({"kind": "finished", "include_ratings": false})),
    )
    .await;
    let share = r.body["id"].as_str().unwrap().to_string();
    let b = |p: &str| {
        p.replace("{b}", &book)
            .replace("{c}", &copy)
            .replace("{s}", &share)
    };
    // Every route in v1::routes, with the scope it needs.
    let table: Vec<(&str, String, Option<Value>, &str)> = vec![
        ("GET", b("/api/v1/profile"), None, "profile:read"),
        ("GET", b("/api/v1/books"), None, "books:read"),
        (
            "POST",
            b("/api/v1/books"),
            Some(json!({"manual": {"title": "X", "authors": ["Y"]}})),
            "books:write",
        ),
        ("GET", b("/api/v1/books/{b}"), None, "books:read"),
        (
            "PATCH",
            b("/api/v1/books/{b}"),
            Some(json!({"title": "X"})),
            "books:write",
        ),
        (
            "PATCH",
            b("/api/v1/books/{b}"),
            Some(json!({"rating": 3})),
            "ratings:write",
        ),
        (
            "PATCH",
            b("/api/v1/books/{b}"),
            Some(json!({"notes": "x"})),
            "notes:write",
        ),
        ("DELETE", b("/api/v1/books/{b}"), None, "books:delete"),
        ("PUT", b("/api/v1/books/{b}/tags/x"), None, "tags:write"),
        ("DELETE", b("/api/v1/books/{b}/tags/x"), None, "tags:write"),
        (
            "POST",
            b("/api/v1/books/{b}/copies"),
            Some(json!({"format": "Ebook"})),
            "copies:write",
        ),
        (
            "PATCH",
            b("/api/v1/copies/{c}"),
            Some(json!({"location": "x"})),
            "copies:write",
        ),
        ("DELETE", b("/api/v1/copies/{c}"), None, "copies:write"),
        ("GET", b("/api/v1/books/{b}/readings"), None, "reading:read"),
        ("POST", b("/api/v1/books/{b}/start"), None, "reading:write"),
        ("POST", b("/api/v1/books/{b}/pause"), None, "reading:write"),
        ("POST", b("/api/v1/books/{b}/finish"), None, "reading:write"),
        (
            "POST",
            b("/api/v1/books/{b}/progress"),
            Some(json!({"position": 1})),
            "reading:write",
        ),
        (
            "GET",
            b("/api/v1/catalog/search?q=dune"),
            None,
            "catalog:search",
        ),
        ("GET", b("/api/v1/shares"), None, "shares:read"),
        (
            "POST",
            b("/api/v1/shares"),
            Some(json!({"kind": "active", "include_ratings": false})),
            "shares:write",
        ),
        (
            "PATCH",
            b("/api/v1/shares/{s}"),
            Some(json!({"new_link": true})),
            "shares:write",
        ),
        ("DELETE", b("/api/v1/shares/{s}"), None, "shares:write"),
    ];
    // A token with every other scope: each request is refused for its own.
    for (m, p, body, needed) in &table {
        let others: Vec<&str> = super::scope::ALL
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| {
                *k != *needed
                    && !(needed.ends_with(":read") && *k == needed.replace(":read", ":write"))
            })
            .collect();
        let tok = pat(&app, &cookie, &others).await;
        let r = api(&app, m, p, &tok, body.clone()).await;
        assert_eq!(
            r.status,
            StatusCode::FORBIDDEN,
            "{m} {p} without {needed}: {}",
            r.text
        );
        assert_eq!(r.body["error"], "insufficient_scope", "{m} {p}");
        let challenge = r.headers[header::WWW_AUTHENTICATE].to_str().unwrap();
        assert!(
            challenge.contains(&format!("scope=\"{needed}\"")),
            "{m} {p}: {challenge}"
        );
        // Revoke it again so the reader stays under the token limit.
        sqlx::query("DELETE FROM api_tokens WHERE token_hash = ?")
            .bind(crate::util::sha256(&tok))
            .execute(&app.state.db)
            .await
            .unwrap();
    }
    // Nothing was changed by any refused request.
    let r = api(&app, "GET", &format!("/api/v1/books/{book}"), &t, None).await;
    assert_eq!(r.body["title"], "Dune");
    assert_eq!(r.body["state"], "inactive");
    assert_eq!(r.body["copies"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_paths_and_bad_bodies_answer_clearly() {
    let app = TestApp::new().await;
    let (_, t, book, _) = library_with_one_book(&app).await;
    let r = api(&app, "GET", "/api/v1/nope", &t, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.body["error"], "not_found");
    assert_eq!(r.headers["access-control-allow-origin"], "*");
    for body in [
        json!({}),
        json!({"manual": {"title": "A", "authors": ["B"]}, "work_id": "x"}),
    ] {
        let r = api(&app, "POST", "/api/v1/books", &t, Some(body.clone())).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "", "authors": ["B"]}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "a title is required");
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "A", "authors": []}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "an author is required");
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{book}"),
        &t,
        Some(json!({"colour": "red"})),
    )
    .await;
    assert!(r.status.is_client_error(), "unknown fields are refused");
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{book}"),
        &t,
        Some(json!({"rating": 6})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    // Adding a book that's already on the list.
    let work = api(&app, "GET", &format!("/api/v1/books/{book}"), &t, None)
        .await
        .body["work_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"work_id": work})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(r.body["message"].as_str().unwrap().contains(&book));
    // Tags on a new book need tags:write.
    let (_, cookie2) = app.user("juno", false).await;
    let w = pat(&app, &cookie2, &["books:write"]).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &w,
        Some(json!({"manual": {"title": "A", "authors": ["B"]}, "tags": ["x"]})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Without books:read (notes:write alone), a change answers just ok.
    let n = pat(&app, &cookie2, &["notes:write"]).await;
    let w = pat(&app, &cookie2, &["books:write"]).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &w,
        Some(json!({"manual": {"title": "A", "authors": ["B"]}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.body["id"].as_str().unwrap();
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/books/{id}"),
        &n,
        Some(json!({"notes": "secret"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body, json!({"ok": true}));
}

#[tokio::test]
async fn editing_books_and_copies() {
    let app = TestApp::new().await;
    let (_, t, book, copy) = library_with_one_book(&app).await;
    let path = format!("/api/v1/books/{book}");
    let r = api(
        &app,
        "PATCH",
        &path,
        &t,
        Some(json!({"title": "Dune (1965)", "authors": ["F. Herbert"], "rating": 5})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["title"], "Dune (1965)");
    assert_eq!(r.body["authors"], json!(["F. Herbert"]));
    assert_eq!(r.body["rating"], 5);
    // "" and [] go back to the catalog's; a rating of 0 clears it.
    let r = api(
        &app,
        "PATCH",
        &path,
        &t,
        Some(json!({"title": "", "authors": [], "rating": 0})),
    )
    .await;
    assert_eq!(r.body["title"], "Dune");
    assert_eq!(r.body["authors"], json!(["Frank Herbert"]));
    assert_eq!(r.body["rating"], Value::Null);
    // Copies: change the edition to unknown, delete, and only your own.
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/copies/{copy}"),
        &t,
        Some(json!({"format": "Hardcover", "edition_id": ""})),
    )
    .await;
    assert_eq!(r.body["format"], "Hardcover");
    let (_, other) = app.user("juno", false).await;
    let o = pat(&app, &other, ALL).await;
    for (m, body) in [
        ("PATCH", Some(json!({"location": "mine"}))),
        ("DELETE", None),
    ] {
        let r = api(&app, m, &format!("/api/v1/copies/{copy}"), &o, body).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{m}");
    }
    let r = api(&app, "DELETE", &format!("/api/v1/copies/{copy}"), &t, None).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/copies/{copy}"),
        &t,
        Some(json!({"location": "x"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = api(&app, "GET", &path, &t, None).await;
    assert_eq!(r.body["copies"], json!([]));
}

#[tokio::test]
async fn starting_by_percent_and_changing_the_page_count() {
    let app = TestApp::new().await;
    let (_, t, book, _) = library_with_one_book(&app).await;
    // No page count known and none sent: pages can't start, percent can.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/start"),
        &t,
        Some(json!({"unit": "pages"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/start"),
        &t,
        Some(json!({"unit": "lines"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/start"),
        &t,
        None,
    )
    .await;
    assert_eq!(r.body["reading"]["unit"], "percent");
    assert_eq!(r.body["reading"]["total"], 100);
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/progress"),
        &t,
        Some(json!({"total": 300})),
    )
    .await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "a percent reading's total is 100"
    );
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/progress"),
        &t,
        Some(json!({"add": 30})),
    )
    .await;
    assert_eq!(r.body["reading"]["position"], 30);
    // Pausing an inactive book changes nothing; pausing this one does.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/pause"),
        &t,
        None,
    )
    .await;
    assert_eq!(r.body["state"], "inactive");
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/pause"),
        &t,
        None,
    )
    .await;
    assert_eq!(r.body["state"], "inactive");

    // A pages reading takes a new page count with or without a move.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Emma", "authors": ["Jane Austen"]}})),
    )
    .await;
    let emma = r.body["id"].as_str().unwrap().to_string();
    api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma}/start"),
        &t,
        Some(json!({"total": 400})),
    )
    .await;
    api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma}/progress"),
        &t,
        Some(json!({"position": 100})),
    )
    .await;
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma}/progress"),
        &t,
        Some(json!({"total": 450})),
    )
    .await;
    assert_eq!(r.body["reading"]["total"], 450);
    assert_eq!(r.body["reading"]["position"], 100);
    // Lowering the total below the position finishes the book there.
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma}/progress"),
        &t,
        Some(json!({"total": 80})),
    )
    .await;
    assert_eq!(r.body["state"], "finished");
    assert_eq!(r.body["reading"]["position"], 80);
    let r = api(&app, "GET", "/api/v1/books?state=inactive", &t, None).await;
    let titles: Vec<&str> = r.body["books"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Dune"]);
}

#[tokio::test]
async fn catalog_search_and_adding_a_result() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, &["catalog:search", "books:write"]).await;
    // Open Library's answer, as if fetched already (no network in tests).
    let found = json!([{
        "provider": "openlibrary", "source_id": "OL893415W", "title": "Dune",
        "authors": ["Frank Herbert"], "first_year": 1965, "cover_url": null,
        "edition_count": 1, "editions": []
    }]);
    crate::providers::cache_put(&app.state.db, "search:openlibrary:dune", &found).await;
    crate::providers::cache_put(&app.state.db, "editions:openlibrary:OL893415W", &json!([])).await;
    let r = api(&app, "GET", "/api/v1/catalog/search?q=Dune", &t, None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["results"][0]["source_id"], "OL893415W");
    assert_eq!(r.body["results"][0]["book_id"], Value::Null);
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let r = api(&app, "GET", "/api/v1/catalog/search?q=Dune", &t, None).await;
    assert!(
        r.body["results"][0]["book_id"].is_string(),
        "now on the list"
    );
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // Only results mau has seen can be added: an app can't invent one.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL1W"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = api(&app, "GET", "/api/v1/catalog/search?q=", &t, None).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    // The work went into the shared catalog, from the provider's record.
    let shared: (String, Option<String>) =
        sqlx::query_as("SELECT title, owner_id FROM works WHERE source_id = 'OL893415W'")
            .fetch_one(&app.state.db)
            .await
            .unwrap();
    assert_eq!(shared, ("Dune".to_string(), None));
}

#[tokio::test]
async fn shares_through_the_api() {
    let app = TestApp::new().await;
    let (cookie, t, _, _) = library_with_one_book(&app).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/shares",
        &t,
        Some(json!({"kind": "catalog", "include_ratings": true})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let id = r.body["id"].as_str().unwrap().to_string();
    let link = r.body["token"].as_str().unwrap().to_string();
    let r = api(
        &app,
        "POST",
        "/api/v1/shares",
        &t,
        Some(json!({"kind": "catalog", "include_ratings": false})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = api(
        &app,
        "POST",
        "/api/v1/shares",
        &t,
        Some(json!({"kind": "everything", "include_ratings": false})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/shares/{id}"),
        &t,
        Some(json!({"new_link": true, "include_ratings": false})),
    )
    .await;
    assert_ne!(r.body["token"], link);
    assert_eq!(r.body["include_ratings"], false);
    let reader = pat(&app, &cookie, &["shares:read"]).await;
    let r = api(&app, "GET", "/api/v1/shares", &reader, None).await;
    assert_eq!(r.body.as_array().unwrap().len(), 1);
    // The app's change shows in Settings › Sharing too.
    let (_, shares) = app.get("/api/shares", Some(&cookie)).await;
    assert_eq!(shares[0]["kind"], "catalog");
    // Another reader's share: not found, and untouched.
    let (_, other) = app.user("juno", false).await;
    let o = pat(&app, &other, &["shares:write"]).await;
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/shares/{id}"),
        &o,
        Some(json!({"new_link": true})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    api(&app, "DELETE", &format!("/api/v1/shares/{id}"), &o, None).await;
    assert_eq!(
        api(&app, "GET", "/api/v1/shares", &reader, None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    api(&app, "DELETE", &format!("/api/v1/shares/{id}"), &t, None).await;
    assert_eq!(
        api(&app, "GET", "/api/v1/shares", &reader, None).await.body,
        json!([])
    );
}

#[tokio::test]
async fn expired_tokens_are_refused() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let personal = pat(&app, &cookie, &["profile:read"]).await;
    let oauth = connect(&app, &cookie, "profile:read").await;
    let access = oauth["access_token"].as_str().unwrap();
    let refresh = oauth["refresh_token"].as_str().unwrap();
    sqlx::query("UPDATE api_tokens SET expires_at = 1")
        .execute(&app.state.db)
        .await
        .unwrap();
    for t in [personal.as_str(), access] {
        let r = api(&app, "GET", "/api/v1/profile", t, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.body["error"], "invalid_token");
    }
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", APP),
        ],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_grant");
    let (_, list) = app.get("/api/api-access", Some(&cookie)).await;
    assert_eq!(list["tokens"], json!([]), "expired tokens aren't listed");
}

#[tokio::test]
async fn the_token_endpoint_in_detail() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", APP),
            ("client_secret", "s"),
        ],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["error"], "invalid_client");
    let r = form(
        &app,
        "/oauth/token",
        &[("grant_type", "authorization_code"), ("client_id", APP)],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_request");
    assert!(
        r.body["error_description"]
            .as_str()
            .unwrap()
            .contains("code")
    );
    let r = form(&app, "/oauth/token", &[("grant_type", "refresh_token")]).await;
    assert_eq!(r.body["error"], "invalid_request");
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    // A refresh may narrow the scopes; the new token has only those.
    let tokens = connect(&app, &cookie, "profile:read books:read").await;
    let r = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
            ("client_id", APP),
            ("scope", "books:read"),
        ],
    )
    .await;
    assert_eq!(r.body["scope"], "books:read");
    let narrow = r.body["access_token"].as_str().unwrap();
    assert_eq!(
        api(&app, "GET", "/api/v1/profile", narrow, None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api(&app, "GET", "/api/v1/books", narrow, None).await.status,
        StatusCode::OK
    );
    // The discovery document lists every scope and nothing else.
    let r = call(
        &app,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[],
        None,
    )
    .await;
    let listed: Vec<&str> = r.body["scopes_supported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let all: Vec<&str> = super::scope::ALL.iter().map(|(k, _)| *k).collect();
    assert_eq!(listed, all);
    assert_eq!(
        r.body["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
}

#[tokio::test]
async fn a_reader_may_have_fifty_tokens_and_admins_can_only_block_known_apps() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("root", true).await;
    for _ in 0..50 {
        pat(&app, &admin, &["profile:read"]).await;
    }
    let (status, body) = app
        .post(
            "/api/api-access/tokens",
            json!({"name": "one more", "scopes": ["profile:read"], "expires_in_days": 30}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, _) = app
        .post(
            "/api/admin/apps/block",
            json!({"client_id": "https://never.example/", "blocked": true}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Editions, a copy when adding, and adding by ISBN

/// Open Library's answers for Dune, as if fetched already (no network in
/// tests): the work, its editions, and an ISBN lookup.
async fn seed_dune(app: &TestApp) {
    let cand = json!({
        "provider": "openlibrary", "source_id": "OL893415W", "title": "Dune",
        "authors": ["Frank Herbert"], "first_year": 1965, "cover_url": null,
        "edition_count": 2, "editions": []
    });
    crate::providers::cache_put(&app.state.db, "search:openlibrary:dune", &json!([cand])).await;
    crate::providers::cache_put(&app.state.db, "isbn:openlibrary:9780441013593", &cand).await;
    crate::providers::cache_put(
        &app.state.db,
        "isbn:openlibrary:9780000000002",
        &Value::Null,
    )
    .await;
    crate::providers::cache_put(
        &app.state.db,
        "editions:openlibrary:OL893415W",
        &json!([
            {"source_id": "OL1M", "publisher": "Ace", "binding": "Paperback", "year": 2005, "isbn13": "9780441013593", "pages": 528, "cover_url": null},
            {"source_id": "OL2M", "publisher": "Chilton", "binding": "Hardcover", "year": 1965, "isbn13": "9780801950773", "pages": 412, "cover_url": null}
        ]),
    )
    .await;
}

#[tokio::test]
async fn books_list_their_editions_for_copies() {
    let app = TestApp::new().await;
    seed_dune(&app).await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    api(&app, "GET", "/api/v1/catalog/search?q=dune", &t, None).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let eds = r.body["editions"].as_array().unwrap();
    assert_eq!(eds.len(), 2);
    let hard = eds.iter().find(|e| e["isbn13"] == "9780801950773").unwrap();
    assert_eq!(hard["binding"], "Hardcover");
    assert_eq!(hard["pages"], 412);
    // A copy can point at one of them.
    let book = r.body["id"].as_str().unwrap();
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{book}/copies"),
        &t,
        Some(json!({"format": "Hardcover", "edition_id": hard["id"]})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["edition_id"], hard["id"]);
    // Editions show with books:read alone (no copies:read needed).
    let reader = pat(&app, &cookie, &["books:read"]).await;
    let r = api(&app, "GET", &format!("/api/v1/books/{book}"), &reader, None).await;
    assert_eq!(r.body["editions"].as_array().unwrap().len(), 2);
    assert!(r.body.get("copies").is_none());
    // A manual entry has no editions.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Notes", "authors": ["Me"]}})),
    )
    .await;
    assert_eq!(r.body["editions"], json!([]));
}

#[tokio::test]
async fn copies_only_point_at_their_own_books_editions() {
    let app = TestApp::new().await;
    seed_dune(&app).await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    api(&app, "GET", "/api/v1/catalog/search?q=dune", &t, None).await;
    let dune = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W"}})),
    )
    .await
    .body;
    let dune_edition = dune["editions"][0]["id"].as_str().unwrap().to_string();
    let emma = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"manual": {"title": "Emma", "authors": ["Jane Austen"]}})),
    )
    .await
    .body;
    let emma_id = emma["id"].as_str().unwrap();
    // Dune's edition on Emma's copy: refused, through the API…
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma_id}/copies"),
        &t,
        Some(json!({"format": "Paperback", "edition_id": dune_edition})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text);
    assert_eq!(r.body["error"], "edition_id");
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{emma_id}/copies"),
        &t,
        Some(json!({"format": "Paperback"})),
    )
    .await;
    let copy = r.body["id"].as_str().unwrap().to_string();
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/copies/{copy}"),
        &t,
        Some(json!({"edition_id": dune_edition})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api(
        &app,
        "PATCH",
        &format!("/api/v1/copies/{copy}"),
        &t,
        Some(json!({"edition_id": ""})),
    )
    .await;
    assert_eq!(
        r.status,
        StatusCode::OK,
        "an empty edition is the unknown edition"
    );
    // …and through sync it becomes "unknown edition" rather than pointing elsewhere.
    let now = crate::util::now_ms();
    let hlc = format!("{now:013}-0000-dev");
    let (status, body) = app
        .post(
            "/api/sync",
            json!({"since": 0, "ops": [
                {"id": "o1", "hlc": hlc, "op": {"type": "copy.create", "id": "c-sync", "book_id": emma_id, "format": "Ebook", "location": "", "edition_id": dune_edition}}
            ]}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let synced = body["changes"]["copies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "c-sync")
        .unwrap();
    assert_eq!(synced["edition_id"], Value::Null);
    // Another reader's private edition is out of reach too.
    let (_, other) = app.user("juno", false).await;
    let o = pat(&app, &other, ALL).await;
    let mine = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"manual": {"title": "Juno's", "authors": ["J"]}})),
    )
    .await
    .body;
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{}/copies", mine["id"].as_str().unwrap()),
        &o,
        Some(json!({"format": "Ebook", "edition_id": dune_edition})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn adding_by_isbn_with_a_copy() {
    let app = TestApp::new().await;
    seed_dune(&app).await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    // Not yet in the shared catalog: looked up at the provider (ISBN-10 is fine).
    let r = api(&app, "POST", "/api/v1/books", &t, Some(json!({"isbn": "0-441-01359-7", "copy": {"format": "Paperback", "location": "Shelf 2"}}))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["title"], "Dune");
    let ace = r.body["editions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["isbn13"] == "9780441013593")
        .unwrap()["id"]
        .clone();
    assert_eq!(
        r.body["copies"][0]["edition_id"], ace,
        "the copy is the edition with that ISBN"
    );
    assert_eq!(r.body["copies"][0]["location"], "Shelf 2");
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"isbn": "9780441013593"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "already on the list");

    // Now in the shared catalog: another reader gets the same work, no provider asked.
    sqlx::query("DELETE FROM provider_cache")
        .execute(&app.state.db)
        .await
        .unwrap();
    let (_, other) = app.user("juno", false).await;
    let o = pat(&app, &other, &["books:write"]).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"isbn": "9780801950773"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["title"], "Dune");
    // A copy needs copies:write.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"manual": {"title": "X", "authors": ["Y"]}, "copy": {"format": "Ebook"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Bad and unknown ISBNs (the providers' answers seeded again: no network).
    seed_dune(&app).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"isbn": "9780441013594"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "check digit");
    assert_eq!(r.body["error"], "isbn");
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"isbn": "9780000000002"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text);
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"isbn": "9780441013593", "manual": {"title": "A", "authors": ["B"]}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "one way at a time");
    // An explicit edition must be the book's; a wrong one changes nothing.
    let before = api(&app, "GET", "/api/v1/books", &t, None).await.body["books"]
        .as_array()
        .unwrap()
        .len();
    let r = api(&app, "POST", "/api/v1/books", &t, Some(json!({"manual": {"title": "Emma", "authors": ["Jane Austen"]}, "copy": {"format": "Ebook", "edition_id": ace}}))).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let after = api(&app, "GET", "/api/v1/books", &t, None).await.body["books"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(before, after, "the whole add was refused");
}

#[tokio::test]
async fn editions_for_copies_in_detail() {
    let app = TestApp::new().await;
    seed_dune(&app).await;
    let (_, cookie) = app.user("rri", false).await;
    let t = pat(&app, &cookie, ALL).await;
    // A mistyped ISBN on a catalog result is refused, not ignored.
    api(&app, "GET", "/api/v1/catalog/search?q=dune", &t, None).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W", "isbn": "9780441013594"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text);
    assert_eq!(r.body["error"], "isbn");
    // A catalog add with an ISBN: the copy gets that edition.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &t,
        Some(json!({"catalog": {"provider": "openlibrary", "source_id": "OL893415W", "isbn": "9780801950773"}, "copy": {"format": "Hardcover"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    let work = r.body["work_id"].as_str().unwrap().to_string();
    let hard = r.body["editions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["isbn13"] == "9780801950773")
        .unwrap()["id"]
        .clone();
    assert_eq!(r.body["copies"][0]["edition_id"], hard);

    // Another reader's private edition of the same work: theirs to use, not mine.
    let (juno_id, other) = app.user("juno", false).await;
    sqlx::query(
        "INSERT INTO editions (id, work_id, publisher, binding, year, isbn13, pages, owner_id, source, seq)
         VALUES ('ed-juno', ?, 'Juno Press', 'Paperback', 2020, NULL, 300, ?, 'manual', 0)",
    )
    .bind(&work)
    .bind(&juno_id)
    .execute(&app.state.db)
    .await
    .unwrap();
    let o = pat(&app, &other, ALL).await;
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"work_id": work, "copy": {"format": "Paperback", "edition_id": "ed-juno"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
    assert_eq!(r.body["copies"][0]["edition_id"], "ed-juno");
    assert_eq!(r.body["editions"].as_array().unwrap().len(), 3);
    let mine = api(&app, "GET", "/api/v1/books", &t, None).await.body["books"][0].clone();
    assert_eq!(
        mine["editions"].as_array().unwrap().len(),
        2,
        "private editions stay private"
    );
    let r = api(
        &app,
        "POST",
        &format!("/api/v1/books/{}/copies", mine["id"].as_str().unwrap()),
        &t,
        Some(json!({"format": "Paperback", "edition_id": "ed-juno"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text);

    // Looking an ISBN up at the providers counts as a search (30 a minute).
    for _ in 0..30 {
        let r = api(
            &app,
            "POST",
            "/api/v1/books",
            &o,
            Some(json!({"isbn": "9780000000002"})),
        )
        .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text);
    }
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"isbn": "9780000000002"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // An ISBN already in the shared catalog asks no provider, so isn't counted.
    let r = api(
        &app,
        "POST",
        "/api/v1/books",
        &o,
        Some(json!({"isbn": "9780441013593"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text);
}
