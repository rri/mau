//! Who may call what: every endpoint signed out and as a regular user, the
//! confirmation admins need for account-takeover powers, and the privacy of
//! sessions, shares, public pages, exports, and the audit log.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app_tests::TestApp;

/// Every API endpoint that needs a signed-in user (method, path).
const SIGNED_IN: &[(&str, &str)] = &[
    ("POST", "/api/auth/confirm"),
    ("PATCH", "/api/account"),
    ("DELETE", "/api/account"),
    ("PUT", "/api/account/password"),
    ("GET", "/api/account/google-books"),
    ("PUT", "/api/account/google-books"),
    ("DELETE", "/api/account/google-books"),
    ("DELETE", "/api/two-factor"),
    ("POST", "/api/two-factor/setup"),
    ("POST", "/api/two-factor/enable"),
    ("GET", "/api/sessions"),
    ("POST", "/api/sessions/revoke-others"),
    ("DELETE", "/api/sessions/x"),
    ("GET", "/api/recovery-codes"),
    ("POST", "/api/recovery-codes/regenerate"),
    ("POST", "/api/sync"),
    ("GET", "/api/catalog/search?q=dune"),
    (
        "GET",
        "/api/catalog/editions?provider=openlibrary&source_id=x",
    ),
    ("POST", "/api/catalog/ensure"),
    ("GET", "/api/catalog/similar?title=dune"),
    ("GET", "/api/catalog/thumb?u=https://example.com/a.jpg"),
    ("GET", "/api/catalog/works/x"),
    ("POST", "/api/catalog/works/x/suggest"),
    ("GET", "/api/shares"),
    ("POST", "/api/shares"),
    ("PATCH", "/api/shares/x"),
    ("DELETE", "/api/shares/x"),
    ("GET", "/api/export.json"),
    ("GET", "/api/export.csv"),
    ("POST", "/api/import/preview"),
    ("POST", "/api/import"),
    ("GET", "/api/import/current"),
    ("GET", "/api/import/x"),
    ("DELETE", "/api/import/x"),
    ("POST", "/api/import/x/rows/0"),
    ("POST", "/api/import/x/commit"),
    ("GET", "/api/invites"),
    ("POST", "/api/invites"),
    ("DELETE", "/api/invites/x"),
    ("POST", "/api/covers"),
];

/// Admin-only endpoints.
const ADMIN: &[(&str, &str)] = &[
    ("GET", "/api/admin/users"),
    ("PATCH", "/api/admin/users/x"),
    ("DELETE", "/api/admin/users/x"),
    ("POST", "/api/admin/users/x/reset-link"),
    ("GET", "/api/admin/signup"),
    ("PUT", "/api/admin/signup"),
    ("GET", "/api/admin/providers"),
    ("PUT", "/api/admin/providers"),
    ("GET", "/api/admin/queue"),
    ("POST", "/api/admin/queue/x/promote"),
    ("POST", "/api/admin/queue/x/merge"),
    ("POST", "/api/admin/queue/x/decline"),
    ("GET", "/api/admin/catalog?q=x"),
    ("PUT", "/api/admin/catalog/works/x"),
    ("POST", "/api/admin/catalog/works/x/cover"),
    ("PUT", "/api/admin/catalog/editions/x"),
    ("GET", "/api/admin/audit"),
    ("GET", "/api/admin/stats"),
];

/// A session that hasn't confirmed its password recently.
async fn unconfirmed(app: &TestApp, user_id: &str) -> String {
    crate::auth::session::create(&app.state.db, user_id, "Other device", false)
        .await
        .unwrap()
}

async fn html(app: &TestApp, path: &str) -> (StatusCode, String) {
    let res = crate::app::router(app.state.clone())
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn book(app: &TestApp, user_id: &str, id: &str, title: &str, rating: i64, notes: &str) {
    sqlx::query("INSERT INTO works (id, title, created_at, seq) VALUES (?, ?, 0, 1)")
        .bind(format!("w-{id}"))
        .bind(title)
        .execute(&app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO books (id, user_id, work_id, rank, rating, notes, added_at, seq) VALUES (?, ?, ?, 'V', ?, ?, 0, 1)")
        .bind(id)
        .bind(user_id)
        .bind(format!("w-{id}"))
        .bind(rating)
        .bind(notes)
        .execute(&app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO readings (id, book_id, user_id, started_on, finished_on, total, pos, seq) VALUES (?, ?, ?, '2026-01-01', '2026-02-01', 100, 100, 1)")
        .bind(format!("r-{id}"))
        .bind(id)
        .bind(user_id)
        .execute(&app.state.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn signed_out_requests_are_refused_everywhere() {
    let app = TestApp::new().await;
    for (method, path) in SIGNED_IN.iter().chain(ADMIN) {
        let (status, body, _) = app.request(method, path, None, None).await;
        assert_eq!(status, 401, "{method} {path}: {body}");
    }
}

#[tokio::test]
async fn regular_users_cant_use_admin_endpoints() {
    let app = TestApp::new().await;
    let (_, user) = app.user("juno", false).await;
    for (method, path) in ADMIN {
        let (status, body, _) = app.request(method, path, None, Some(&user)).await;
        assert_eq!(status, 403, "{method} {path}: {body}");
    }
}

#[tokio::test]
async fn admin_takeover_powers_need_a_recent_confirmation() {
    let app = TestApp::new().await;
    let (admin_id, _) = app.user("boss", true).await;
    let (juno, _) = app.user("juno", false).await;
    let admin = unconfirmed(&app, &admin_id).await;
    for (method, path, body) in [
        ("POST", format!("/api/admin/users/{juno}/reset-link"), None),
        ("DELETE", format!("/api/admin/users/{juno}"), None),
        (
            "PATCH",
            format!("/api/admin/users/{juno}"),
            Some(json!({"is_admin": true})),
        ),
    ] {
        let (status, res, _) = app.request(method, &path, body, Some(&admin)).await;
        assert_eq!(status, 403, "{method} {path}");
        assert_eq!(res["error"], "reauth_required", "{method} {path}");
    }
    // Lesser changes don't ask.
    let (status, _, _) = app
        .request(
            "PATCH",
            &format!("/api/admin/users/{juno}"),
            Some(json!({"suspended": true})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, 200);
    // After confirming, the reset link is issued.
    let (status, _) = app
        .post(
            "/api/auth/confirm",
            json!({"password": crate::app_tests::PASSWORD, "code": null}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, 200);
    let (status, link) = app
        .post(
            &format!("/api/admin/users/{juno}/reset-link"),
            json!({}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, 200);
    assert!(link["url"].as_str().unwrap().contains("/reset/"));
}

#[tokio::test]
async fn sessions_are_listed_and_signed_out_only_for_their_owner() {
    let app = TestApp::new().await;
    let (rri, me) = app.user("rri", false).await;
    let other = unconfirmed(&app, &rri).await;
    let (_, juno) = app.user("juno", false).await;
    let (_, list) = app.get("/api/sessions", Some(&me)).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|s| s["current"] == true).count(), 1);
    let other_id = list.iter().find(|s| s["current"] == false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Someone else can't sign it out.
    app.request(
        "DELETE",
        &format!("/api/sessions/{other_id}"),
        None,
        Some(&juno),
    )
    .await;
    assert_eq!(app.get("/api/sessions", Some(&other)).await.0, 200);
    // Its owner can; this device stays signed in.
    app.request(
        "DELETE",
        &format!("/api/sessions/{other_id}"),
        None,
        Some(&me),
    )
    .await;
    assert_eq!(app.get("/api/sessions", Some(&other)).await.0, 401);
    assert_eq!(app.get("/api/sessions", Some(&me)).await.0, 200);
    // Sign out all others.
    let third = unconfirmed(&app, &rri).await;
    app.post("/api/sessions/revoke-others", json!({}), Some(&me))
        .await;
    assert_eq!(app.get("/api/sessions", Some(&third)).await.0, 401);
    assert_eq!(app.get("/api/sessions", Some(&me)).await.0, 200);
}

#[tokio::test]
async fn shares_publish_only_what_they_name_and_never_notes() {
    let app = TestApp::new().await;
    let (rri, me) = app.user("rri", false).await;
    let (_, juno) = app.user("juno", false).await;
    for i in 0..10 {
        book(
            &app,
            &rri,
            &format!("b{i}"),
            &format!("Book {i}"),
            4,
            "secret note",
        )
        .await;
    }
    let (status, share) = app
        .post(
            "/api/shares",
            json!({"kind": "finished", "tag": null, "include_ratings": false}),
            Some(&me),
        )
        .await;
    assert_eq!(status, 200, "{share}");
    let token = share["token"].as_str().unwrap().to_string();

    // The share page: titled once, all ten books, no ratings, no notes.
    let (status, page) = html(&app, &format!("/s/{token}")).await;
    assert_eq!(status, 200);
    assert_eq!(page.matches("Finished").count(), 2, "title + h1 only");
    assert!(!page.contains("<h2"));
    assert!(page.contains("Book 9"));
    assert!(!page.contains('★'));
    assert!(!page.contains("secret note"));

    // The profile previews eight with a link to all of them.
    let (_, profile) = html(&app, "/u/rri").await;
    assert!(profile.contains("See all 10"));

    // Others can't change or remove it.
    let id = share["id"].as_str().unwrap();
    let (status, _, _) = app
        .request(
            "PATCH",
            &format!("/api/shares/{id}"),
            Some(json!({"include_ratings": true})),
            Some(&juno),
        )
        .await;
    assert_eq!(status, 404);
    app.request("DELETE", &format!("/api/shares/{id}"), None, Some(&juno))
        .await;
    assert_eq!(html(&app, &format!("/s/{token}")).await.0, 200);

    // Showing ratings, then a new link: the old one stops working.
    let (_, s, _) = app
        .request(
            "PATCH",
            &format!("/api/shares/{id}"),
            Some(json!({"include_ratings": true, "new_link": true})),
            Some(&me),
        )
        .await;
    let new = s["token"].as_str().unwrap();
    assert_ne!(new, token);
    assert!(
        html(&app, &format!("/s/{token}"))
            .await
            .1
            .contains("Nothing here")
    );
    assert!(html(&app, &format!("/s/{new}")).await.1.contains("★★★★"));

    // A suspended owner's pages disappear.
    sqlx::query("UPDATE users SET suspended = 1 WHERE id = ?")
        .bind(&rri)
        .execute(&app.state.db)
        .await
        .unwrap();
    assert!(
        html(&app, &format!("/s/{new}"))
            .await
            .1
            .contains("Nothing here")
    );
    assert!(html(&app, "/u/rri").await.1.contains("Nothing here"));
}

#[tokio::test]
async fn a_short_list_has_no_see_all_link() {
    let app = TestApp::new().await;
    let (rri, me) = app.user("rri", false).await;
    book(&app, &rri, "b1", "Only One", 0, "").await;
    app.post(
        "/api/shares",
        json!({"kind": "finished", "tag": null, "include_ratings": false}),
        Some(&me),
    )
    .await;
    let (_, profile) = html(&app, "/u/rri").await;
    assert!(profile.contains("Only One"));
    assert!(!profile.contains("See all"));
}

#[tokio::test]
async fn exports_hold_only_your_own_books() {
    let app = TestApp::new().await;
    let (rri, me) = app.user("rri", false).await;
    let (juno_id, _) = app.user("juno", false).await;
    book(&app, &rri, "mine", "Mine", 3, "my note").await;
    book(&app, &juno_id, "theirs", "Theirs", 3, "their note").await;
    let (status, json) = app.get("/api/export.json", Some(&me)).await;
    assert_eq!(status, 200);
    let text = json.to_string();
    assert!(text.contains("Mine") && text.contains("my note"));
    assert!(!text.contains("Theirs") && !text.contains("their note"));
    let (status, csv) = html_with_cookie(&app, "/api/export.csv", &me).await;
    assert_eq!(status, 200);
    assert!(csv.contains("Mine"));
    assert!(!csv.contains("Theirs"));
}

async fn html_with_cookie(app: &TestApp, path: &str, cookie: &str) -> (StatusCode, String) {
    let res = crate::app::router(app.state.clone())
        .oneshot(
            Request::get(path)
                .header("cookie", format!("mau_session={cookie}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn the_audit_log_pages_back_in_time() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("boss", true).await;
    for i in 0..205 {
        crate::audit::record(&app.state.db, None, format!("thing {i}"))
            .await
            .unwrap();
    }
    let (_, first) = app.get("/api/admin/audit", Some(&admin)).await;
    let first = first.as_array().unwrap();
    assert_eq!(first.len(), 200);
    let oldest = first.last().unwrap()["id"].as_i64().unwrap();
    let (_, rest) = app
        .get(&format!("/api/admin/audit?before={oldest}"), Some(&admin))
        .await;
    let all: Vec<&Value> = first.iter().chain(rest.as_array().unwrap()).collect();
    let actions: std::collections::HashSet<&str> =
        all.iter().map(|e| e["action"].as_str().unwrap()).collect();
    assert_eq!(actions.len(), 205, "no entry skipped or repeated");
}

#[tokio::test]
async fn unknown_pages_load_the_app_and_unknown_profiles_say_nothing_here() {
    let app = TestApp::new().await;
    let (status, page) = html(&app, "/no/such/page").await;
    // The app itself shows its "Page not found" card.
    assert_eq!(status, 200);
    assert!(page.contains("<html") || page.is_empty());
    let (status, page) = html(&app, "/u/nobody").await;
    assert_eq!(status, 404);
    assert!(page.contains("Nothing here"));
}
