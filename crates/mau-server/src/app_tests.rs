//! API tests against a temporary database.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app::{AppState, router};
use crate::config::Config;

pub struct TestApp {
    pub state: AppState,
    _dir: tempfile::TempDir,
}

pub const ORIGIN: &str = "http://localhost:8370";

impl TestApp {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = Config { data_dir: dir.path().to_path_buf(), ..Config::default() };
        let db = crate::db::open(&config.database_path()).await.unwrap();
        let state = AppState::new(db, config).unwrap();
        TestApp { state, _dir: dir }
    }

    pub async fn request(&self, method: &str, path: &str, body: Option<Value>, cookie: Option<&str>) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder().method(method).uri(path).header(header::ORIGIN, ORIGIN);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, format!("mau_session={c}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = router(self.state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let set_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("mau_session="))
            .map(|v| v.split(';').next().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, set_cookie)
    }

    pub async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _) = self.request("GET", path, None, cookie).await;
        (s, v)
    }

    pub async fn post(&self, path: &str, body: Value, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _) = self.request("POST", path, Some(body), cookie).await;
        (s, v)
    }

    /// Creates a user directly (bypassing passkeys) and returns a session cookie.
    pub async fn user(&self, username: &str, admin: bool) -> (String, String) {
        let id = crate::util::new_id();
        sqlx::query("INSERT INTO users (id, username, display_name, time_zone, is_admin, webauthn_id, created_at) VALUES (?, ?, ?, 'UTC', ?, ?, 0)")
            .bind(&id)
            .bind(username)
            .bind(username)
            .bind(admin)
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&self.state.db)
            .await
            .unwrap();
        let token = crate::auth::session::create(&self.state.db, &id, "Test", false, true).await.unwrap();
        (id, token)
    }
}

#[tokio::test]
async fn health_reports_ok() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn unknown_api_path_is_json_404() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/api/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn fresh_instance_requires_setup() {
    let app = TestApp::new().await;
    let (_, body) = app.get("/api/auth/status", None).await;
    assert_eq!(body["setup_required"], true);
    assert_eq!(body["me"], Value::Null);
    assert_eq!(body["signup_mode"], "invite");

    // Without the setup token nobody can register.
    let (status, _) = app
        .post("/api/auth/register/start", json!({"username": "rri", "display_name": "R", "time_zone": "UTC"}), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // With it, registration starts and asks for a discoverable credential.
    *app.state.setup_token.lock().unwrap() = Some("tok".into());
    let (status, body) = app
        .post(
            "/api/auth/register/start",
            json!({"username": "rri", "display_name": "R", "time_zone": "UTC", "setup": "tok"}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["options"]["publicKey"]["authenticatorSelection"]["residentKey"], "required");
}

#[tokio::test]
async fn invite_only_signup_needs_a_live_invite() {
    let app = TestApp::new().await;
    app.user("admin1", true).await;
    let start = |invite: Option<&str>| {
        json!({"username": "juno", "display_name": "Juno", "time_zone": "UTC", "invite": invite})
    };
    let (status, _) = app.post("/api/auth/register/start", start(None), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.post("/api/auth/register/start", start(Some("bogus")), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    sqlx::query("INSERT INTO invites (id, token_hash, created_at, expires_at) VALUES ('i1', ?, 0, ?)")
        .bind(crate::util::sha256("good"))
        .bind(crate::util::now_ms() + 1000 * 60)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (_, info) = app.get("/api/auth/invite/good", None).await;
    assert_eq!(info["valid"], true);
    let (status, _) = app.post("/api/auth/register/start", start(Some("good")), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn username_availability() {
    let app = TestApp::new().await;
    app.user("taken", false).await;
    let check = |u: &str| json!({ "username": u });
    let (_, b) = app.post("/api/auth/username-available", check("Taken"), None).await;
    assert_eq!(b["available"], false);
    let (_, b) = app.post("/api/auth/username-available", check("ok_name"), None).await;
    assert_eq!(b["available"], true);
    let (_, b) = app.post("/api/auth/username-available", check("settings"), None).await;
    assert_eq!(b["available"], false);
}

#[tokio::test]
async fn cross_site_posts_are_rejected() {
    let app = TestApp::new().await;
    let req = Request::post("/api/auth/login/start")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let res = router(app.state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn recovery_code_gives_restricted_session_once() {
    let app = TestApp::new().await;
    let (id, _) = app.user("rri", true).await;
    let mut tx = app.state.db.begin().await.unwrap();
    let codes = crate::auth::recovery::regenerate(&mut tx, &id).await.unwrap();
    tx.commit().await.unwrap();

    let body = json!({"username": "RRI", "code": codes[0].to_uppercase()});
    let (status, res, cookie) = app.request("POST", "/api/auth/recovery", Some(body.clone()), None).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["codes_left"], 9);
    let cookie = cookie.unwrap();

    // Restricted: can see status, can't use the app.
    let (_, st) = app.get("/api/auth/status", Some(&cookie)).await;
    assert_eq!(st["restricted"], true);
    let (status, err) = app.get("/api/passkeys", Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["error"], "enroll_required");

    // The same code doesn't work twice.
    let (status, _, _) = app.request("POST", "/api/auth/recovery", Some(body), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sensitive_actions_need_a_recent_passkey() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    sqlx::query("UPDATE sessions SET reauth_at = 0").execute(&app.state.db).await.unwrap();
    let (status, err) = app.post("/api/recovery-codes/regenerate", json!({}), Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["error"], "reauth_required");
    sqlx::query("UPDATE sessions SET reauth_at = ?").bind(crate::util::now_ms()).execute(&app.state.db).await.unwrap();
    let (status, codes) = app.post("/api/recovery-codes/regenerate", json!({}), Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(codes["codes"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn username_change_reserves_the_old_name() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("oldname", false).await;
    let (status, me, _) = app.request("PATCH", "/api/account", Some(json!({"username": "newname", "bio": "Hi"})), Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["username"], "newname");
    let (_, b) = app.post("/api/auth/username-available", json!({"username": "oldname"}), None).await;
    assert_eq!(b["available"], false);
}

#[tokio::test]
async fn last_passkey_cannot_be_removed() {
    let app = TestApp::new().await;
    let (id, cookie) = app.user("rri", false).await;
    sqlx::query("INSERT INTO passkeys (id, user_id, name, cred_id, data, created_at) VALUES ('p1', ?, 'Key', 'c1', '{}', 0)")
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (status, err, _) = app.request("DELETE", "/api/passkeys/p1", None, Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], "last_passkey");
    let _ = Arc::clone(&app.state.config);
}
