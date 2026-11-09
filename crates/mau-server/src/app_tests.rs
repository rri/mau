//! API tests against a temporary database.

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
pub const PASSWORD: &str = "correct horse battery";

impl TestApp {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        let db = crate::db::open(&config.database_path()).await.unwrap();
        let state = AppState::new(db, config).unwrap();
        TestApp { state, _dir: dir }
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::ORIGIN, ORIGIN);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, format!("mau_session={c}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
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

    /// Creates a user directly, with the password [`PASSWORD`], and returns
    /// its ID and a session cookie.
    pub async fn user(&self, username: &str, admin: bool) -> (String, String) {
        let id = crate::util::new_id();
        sqlx::query("INSERT INTO users (id, username, display_name, time_zone, is_admin, password_hash, created_at) VALUES (?, ?, ?, 'UTC', ?, ?, 0)")
            .bind(&id)
            .bind(username)
            .bind(username)
            .bind(admin)
            .bind(crate::auth::password::hash(PASSWORD).await)
            .execute(&self.state.db)
            .await
            .unwrap();
        let token = crate::auth::session::create(&self.state.db, &id, "Test", true)
            .await
            .unwrap();
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
    let reg =
        json!({"username": "rri", "display_name": "R", "time_zone": "UTC", "password": PASSWORD});
    let (status, _) = app.post("/api/auth/register", reg.clone(), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // With it, the account is created and signed in, with recovery codes.
    *app.state.setup_token.lock().unwrap() = Some("tok".into());
    let mut with_setup = reg.clone();
    with_setup["setup"] = json!("tok");
    with_setup["password"] = json!("short");
    let (status, body) = app
        .post("/api/auth/register", with_setup.clone(), None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "password");
    with_setup["password"] = json!(PASSWORD);
    let (status, body, cookie) = app
        .request("POST", "/api/auth/register", Some(with_setup.clone()), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["codes"].as_array().unwrap().len(), 10);
    let (_, st) = app.get("/api/auth/status", cookie.as_deref()).await;
    assert_eq!(st["me"]["is_admin"], true);
    assert_eq!(st["setup_required"], false);
    // The link works once.
    let (status, _) = app.post("/api/auth/register", with_setup, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invite_only_signup_needs_a_live_invite() {
    let app = TestApp::new().await;
    app.user("admin1", true).await;
    let start = |invite: Option<&str>| json!({"username": "juno", "display_name": "Juno", "time_zone": "UTC", "password": PASSWORD, "invite": invite});
    let (status, _) = app.post("/api/auth/register", start(None), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post("/api/auth/register", start(Some("bogus")), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    sqlx::query(
        "INSERT INTO invites (id, token_hash, created_at, expires_at) VALUES ('i1', ?, 0, ?)",
    )
    .bind(crate::util::sha256("good"))
    .bind(crate::util::now_ms() + 1000 * 60)
    .execute(&app.state.db)
    .await
    .unwrap();
    let (_, info) = app.get("/api/auth/invite/good", None).await;
    assert_eq!(info["valid"], true);
    let (status, _) = app
        .post("/api/auth/register", start(Some("good")), None)
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn username_availability() {
    let app = TestApp::new().await;
    app.user("taken", false).await;
    let check = |u: &str| json!({ "username": u });
    let (_, b) = app
        .post("/api/auth/username-available", check("Taken"), None)
        .await;
    assert_eq!(b["available"], false);
    let (_, b) = app
        .post("/api/auth/username-available", check("ok_name"), None)
        .await;
    assert_eq!(b["available"], true);
    let (_, b) = app
        .post("/api/auth/username-available", check("settings"), None)
        .await;
    assert_eq!(b["available"], false);
}

#[tokio::test]
async fn cross_site_posts_are_rejected() {
    let app = TestApp::new().await;
    let req = Request::post("/api/auth/login")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let res = router(app.state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

async fn login(
    app: &TestApp,
    username: &str,
    password: &str,
) -> (StatusCode, Value, Option<String>) {
    app.request(
        "POST",
        "/api/auth/login",
        Some(json!({"username": username, "password": password})),
        None,
    )
    .await
}

#[tokio::test]
async fn password_sign_in() {
    let app = TestApp::new().await;
    app.user("rri", false).await;
    let (status, body, cookie) = login(&app, "RRI", PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["code_ticket"], Value::Null);
    let (_, st) = app.get("/api/auth/status", cookie.as_deref()).await;
    assert_eq!(st["me"]["username"], "rri");

    let (status, body, cookie) = login(&app, "rri", "wrong password").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "bad_login");
    assert!(cookie.is_none());
    // Unknown accounts get the same answer.
    let (status, body, _) = login(&app, "nobody", PASSWORD).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "bad_login");
}

fn current_code(secret: &str) -> String {
    use hmac::{KeyInit, Mac};
    let key = data_encoding::BASE32_NOPAD
        .decode(secret.as_bytes())
        .unwrap();
    let step = crate::util::now_ms() / 30_000;
    let mut mac = hmac::Hmac::<sha1::Sha1>::new_from_slice(&key).unwrap();
    mac.update(&step.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let o = (h[19] & 0x0f) as usize;
    let n = u32::from_be_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]) & 0x7fff_ffff;
    format!("{:06}", n % 1_000_000)
}

#[tokio::test]
async fn two_factor_setup_and_sign_in() {
    let app = TestApp::new().await;
    let (id, cookie) = app.user("rri", false).await;
    let (status, setup) = app
        .post("/api/two-factor/setup", json!({}), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK, "{setup}");
    assert!(setup["qr_svg"].as_str().unwrap().contains("<svg"));
    let secret = setup["secret"].as_str().unwrap().to_string();
    let (status, _) = app
        .post(
            "/api/two-factor/enable",
            json!({"ticket": setup["ticket"], "code": "000000"}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, me) = app
        .post(
            "/api/two-factor/enable",
            json!({"ticket": setup["ticket"], "code": current_code(&secret)}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["two_factor"], true);

    // The password alone now gives a ticket, not a session.
    let (status, body, session) = login(&app, "rri", PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    assert!(session.is_none());
    let ticket = body["code_ticket"].as_str().unwrap().to_string();
    // The code that turned two-factor on can't be used again.
    let (status, err) = app
        .post(
            "/api/auth/login/code",
            json!({"ticket": ticket, "code": current_code(&secret)}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err["error"], "bad_code");
    // A recovery code works instead.
    let mut tx = app.state.db.begin().await.unwrap();
    let codes = crate::auth::recovery::regenerate(&mut tx, &id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, _, session) = app
        .request(
            "POST",
            "/api/auth/login/code",
            Some(json!({"ticket": ticket, "code": codes[0]})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(session.is_some());
    // The ticket is used up.
    let (status, err) = app
        .post(
            "/api/auth/login/code",
            json!({"ticket": ticket, "code": codes[1]}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], "ticket_expired");
}

#[tokio::test]
async fn too_many_wrong_codes_end_the_sign_in() {
    let app = TestApp::new().await;
    let (id, _) = app.user("rri", false).await;
    sqlx::query("UPDATE users SET totp_secret = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ' WHERE id = ?")
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (_, body, _) = login(&app, "rri", PASSWORD).await;
    let ticket = body["code_ticket"].clone();
    let mut errors = vec![];
    for _ in 0..5 {
        let (_, err) = app
            .post(
                "/api/auth/login/code",
                json!({"ticket": ticket, "code": "zzzzz-zzzzz"}),
                None,
            )
            .await;
        errors.push(err["error"].as_str().unwrap().to_string());
    }
    assert_eq!(errors[3], "bad_code");
    assert_eq!(errors[4], "ticket_expired");
}

#[tokio::test]
async fn recovery_code_sets_a_new_password_once() {
    let app = TestApp::new().await;
    let (id, old_session) = app.user("rri", true).await;
    let mut tx = app.state.db.begin().await.unwrap();
    let codes = crate::auth::recovery::regenerate(&mut tx, &id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let body = json!({"username": "RRI", "code": codes[0].to_uppercase(), "password": "a brand new password"});
    let (status, res, cookie) = app
        .request("POST", "/api/auth/recovery", Some(body.clone()), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["codes_left"], 9);
    assert!(cookie.is_some());
    // Old sessions are signed out; the new password works, the old doesn't.
    let (status, _) = app.get("/api/sessions", Some(&old_session)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        login(&app, "rri", "a brand new password").await.0,
        StatusCode::OK
    );
    assert_eq!(
        login(&app, "rri", PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );

    // The same code doesn't work twice.
    let (status, _, _) = app
        .request("POST", "/api/auth/recovery", Some(body), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn reset_link_sets_a_password_and_turns_off_two_factor() {
    let app = TestApp::new().await;
    let (_, admin) = app.user("boss", true).await;
    let (id, _) = app.user("juno", false).await;
    sqlx::query("UPDATE users SET totp_secret = 'GEZDGNBVGY3TQOJQ' WHERE id = ?")
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (status, link) = app
        .post(
            &format!("/api/admin/users/{id}/reset-link"),
            json!({}),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let token = link["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let (_, info) = app.get(&format!("/api/auth/reset/{token}"), None).await;
    assert_eq!(info["username"], "juno");
    assert_eq!(info["two_factor"], true);
    let path = format!("/api/auth/reset/{token}");
    let (status, _) = app
        .post(&path, json!({"password": "juno's new password"}), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body, session) = login(&app, "juno", "juno's new password").await;
    assert_eq!(body["code_ticket"], Value::Null);
    assert!(session.is_some());
    let (status, _) = app
        .post(&path, json!({"password": "another new password"}), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sensitive_actions_need_a_recent_confirmation() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    sqlx::query("UPDATE sessions SET reauth_at = 0")
        .execute(&app.state.db)
        .await
        .unwrap();
    let (status, err) = app
        .post("/api/recovery-codes/regenerate", json!({}), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["error"], "reauth_required");
    let (status, err) = app
        .post(
            "/api/auth/confirm",
            json!({"password": "nope"}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err["error"], "bad_password");
    let (status, _) = app
        .post(
            "/api/auth/confirm",
            json!({"password": PASSWORD}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, codes) = app
        .post("/api/recovery-codes/regenerate", json!({}), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(codes["codes"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn confirmation_needs_a_code_when_two_factor_is_on() {
    let app = TestApp::new().await;
    let (id, cookie) = app.user("rri", false).await;
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    sqlx::query("UPDATE users SET totp_secret = ? WHERE id = ?")
        .bind(secret)
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (status, err) = app
        .post(
            "/api/auth/confirm",
            json!({"password": PASSWORD}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], "code_required");
    let (status, _) = app
        .post(
            "/api/auth/confirm",
            json!({"password": PASSWORD, "code": current_code(secret)}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, me, _) = app
        .request("DELETE", "/api/two-factor", None, Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["two_factor"], false);
}

#[tokio::test]
async fn username_change_reserves_the_old_name() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("oldname", false).await;
    let (status, me, _) = app
        .request(
            "PATCH",
            "/api/account",
            Some(json!({"username": "newname", "bio": "Hi"})),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["username"], "newname");
    let (_, b) = app
        .post(
            "/api/auth/username-available",
            json!({"username": "oldname"}),
            None,
        )
        .await;
    assert_eq!(b["available"], false);
}

#[tokio::test]
async fn changing_the_password_checks_the_rules() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let (status, err, _) = app
        .request(
            "PUT",
            "/api/account/password",
            Some(json!({"password": "password123"})),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], "password");
    let (status, _, _) = app
        .request(
            "PUT",
            "/api/account/password",
            Some(json!({"password": "a different passphrase"})),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        login(&app, "rri", "a different passphrase").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn recovery_attempts_are_rate_limited() {
    let app = TestApp::new().await;
    app.user("rri", false).await;
    let mut last = StatusCode::OK;
    for _ in 0..6 {
        let (status, _, _) = app
            .request(
                "POST",
                "/api/auth/recovery",
                Some(json!({"username": "rri", "code": "aaaaa-bbbbb", "password": "new password here"})),
                None,
            )
            .await;
        last = status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn responses_carry_security_headers() {
    let app = TestApp::new().await;
    let req = Request::get("/api/health").body(Body::empty()).unwrap();
    let res = router(app.state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.headers()["x-content-type-options"], "nosniff");
    assert_eq!(res.headers()["x-frame-options"], "DENY");
}

#[tokio::test]
async fn wrong_codes_across_sign_ins_lock_the_account() {
    let app = TestApp::new().await;
    let (id, _) = app.user("rri", false).await;
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    sqlx::query("UPDATE users SET totp_secret = ? WHERE id = ?")
        .bind(secret)
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    // Two sign-ins of five wrong codes each: the tenth wrong code locks.
    let mut last = Value::Null;
    for _ in 0..2 {
        let (_, body, _) = login(&app, "rri", PASSWORD).await;
        let ticket = body["code_ticket"].clone();
        for _ in 0..5 {
            last = app
                .post(
                    "/api/auth/login/code",
                    json!({"ticket": ticket, "code": "zzzzz-zzzzz"}),
                    None,
                )
                .await
                .1;
        }
    }
    assert_eq!(last["error"], "codes_locked", "{last}");
    // Even the right code is refused while locked.
    let (_, body, _) = login(&app, "rri", PASSWORD).await;
    let (status, err) = app
        .post(
            "/api/auth/login/code",
            json!({"ticket": body["code_ticket"], "code": current_code(secret)}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err["error"], "codes_locked");
    // The account owner is told.
    let warned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM recovery_events WHERE user_id = ? AND message LIKE '%wrong codes%')",
    )
    .bind(&id)
    .fetch_one(&app.state.db)
    .await
    .unwrap();
    assert!(warned);
    // After the block, the right code works and the count starts over.
    sqlx::query("UPDATE users SET code_locked_until = 0 WHERE id = ?")
        .bind(&id)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (status, err) = app
        .post(
            "/api/auth/login/code",
            json!({"ticket": body["code_ticket"], "code": current_code(secret)}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{err}");
}

#[tokio::test]
async fn a_new_password_signs_out_other_devices() {
    let app = TestApp::new().await;
    let (_, here) = app.user("rri", false).await;
    let (_, _, other) = login(&app, "rri", PASSWORD).await;
    let other = other.unwrap();
    let (status, _, _) = app
        .request(
            "PUT",
            "/api/account/password",
            Some(json!({"password": "a different passphrase"})),
            Some(&here),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, st) = app.get("/api/auth/status", Some(&other)).await;
    assert_eq!(st["me"], Value::Null);
    let (_, st) = app.get("/api/auth/status", Some(&here)).await;
    assert_eq!(st["me"]["username"], "rri");
}

#[tokio::test]
async fn responses_carry_a_content_security_policy() {
    let app = TestApp::new().await;
    let req = Request::get("/api/health").body(Body::empty()).unwrap();
    let res = router(app.state.clone()).oneshot(req).await.unwrap();
    let csp = res.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    // The test instance's public_url is http://localhost: no HSTS.
    assert!(res.headers().get("strict-transport-security").is_none());
}
