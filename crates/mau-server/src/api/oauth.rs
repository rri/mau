//! "Sign in with mau": OAuth 2.1 authorization codes with PKCE, for apps
//! known by their address (see `client.rs`).
//!
//! 1. The app sends the browser to `GET /oauth/authorize?…`. mau checks the
//!    app's address and redirect address (a bad one gets an error page here,
//!    never a redirect), remembers the request for 10 minutes, and sends the
//!    browser to the web app's `/authorize/<request>`.
//! 2. The web app asks the reader to sign in if needed, to confirm the
//!    account, and to allow exactly what the app asked for (with a password
//!    confirmation), then calls `approve` or `deny`, which answer with the
//!    address to send the browser back to: `code`, `state` and `iss`.
//! 3. The app, from its own page, posts the code with its PKCE verifier to
//!    `POST /oauth/token` (CORS allows any origin; no cookies are read) and
//!    gets an access token (1 hour) and a refresh token (30 days, replaced on
//!    every use; using an old one ends the app's access).

use std::collections::HashMap;
use std::sync::Mutex;

use axum::Form;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use maud::{DOCTYPE, PreEscaped, html};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;
use ts_rs::TS;

use super::client::{self, ClientId, Metadata};
use super::scope::{ScopeInfo, Scopes};
use super::token::{self, ACCESS_LIFETIME_MS, Kind, NewToken, REFRESH_LIFETIME_MS};
use crate::app::AppState;
use crate::auth::handlers::Me;
use crate::auth::session::Authed;
use crate::client_ip::ClientIp;
use crate::error::{ApiError, ApiResult};
use crate::util::{MINUTE_MS, new_id, now_ms, random_token, sha256};

const REQUEST_TTL_MS: i64 = 10 * MINUTE_MS;
const CODE_TTL_MS: i64 = MINUTE_MS;
/// Per person and app: live access tokens kept (one per device, roughly),
/// and spent refresh tokens remembered to spot reuse.
const MAX_ACCESS_PER_GRANT: i64 = 20;
const MAX_SPENT_PER_GRANT: i64 = 50;
/// Requests remembered at once; older ones are dropped first.
const MAX_REQUESTS: usize = 10_000;

/// An authorization request waiting for the reader's answer.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub client: ClientId,
    pub name: Option<String>,
    pub redirect_uri: String,
    pub scopes: Scopes,
    pub state: String,
    pub code_challenge: String,
    pub fragment: bool,
}

/// A code the app exchanges for tokens.
#[derive(Debug, Clone)]
struct AuthCode {
    user_id: String,
    grant_id: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    scopes: Scopes,
    expires: i64,
    used: bool,
}

/// Requests and codes live in server memory: a restart means starting the
/// sign-in again, as with two-factor sign-ins.
#[derive(Default)]
pub struct OAuthStore {
    requests: Mutex<HashMap<String, (i64, AuthRequest)>>,
    /// Keyed by the code's SHA-256.
    codes: Mutex<HashMap<String, AuthCode>>,
}

impl OAuthStore {
    fn add_request(&self, r: AuthRequest) -> String {
        let id = random_token();
        let now = now_ms();
        let mut map = self.requests.lock().unwrap();
        map.retain(|_, (exp, _)| *exp > now);
        if map.len() >= MAX_REQUESTS
            && let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (exp, _))| *exp)
                .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
        map.insert(id.clone(), (now + REQUEST_TTL_MS, r));
        id
    }

    fn request(&self, id: &str) -> Option<AuthRequest> {
        let map = self.requests.lock().unwrap();
        map.get(id)
            .filter(|(exp, _)| *exp > now_ms())
            .map(|(_, r)| r.clone())
    }

    fn take_request(&self, id: &str) -> Option<AuthRequest> {
        let r = self.request(id);
        self.requests.lock().unwrap().remove(id);
        r
    }

    fn add_code(&self, code: AuthCode) -> String {
        let secret = random_token();
        let now = now_ms();
        let mut map = self.codes.lock().unwrap();
        map.retain(|_, c| c.expires > now);
        map.insert(sha256(&secret), code);
        secret
    }

    /// The code, marked used. A code used before comes back with `used` set,
    /// so the caller can end the access it led to.
    fn use_code(&self, secret: &str) -> Option<AuthCode> {
        let mut map = self.codes.lock().unwrap();
        let c = map.get_mut(&sha256(secret))?;
        if c.expires <= now_ms() {
            return None;
        }
        let before = c.clone();
        c.used = true;
        Some(before)
    }
}

/// The issuer: this instance's address, as `iss` and in the metadata.
fn issuer(state: &AppState) -> String {
    state.config.public_url.trim_end_matches('/').to_string()
}

// ---------------------------------------------------------------------------
// Step 1: GET /oauth/authorize

#[derive(Deserialize, Default)]
pub struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    response_mode: Option<String>,
}

/// An error shown on mau's own page, for requests that can't safely be sent
/// back to the app (unknown app, or a redirect it may not use).
fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    let font = crate::web::asset_path("instrument-sans-latin-wght-normal");
    let m = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · mau" }
                link rel="icon" href="/icon.svg" type="image/svg+xml";
                @if let Some(f) = &font {
                    style { (PreEscaped(format!("@font-face{{font-family:'Instrument Sans';font-style:normal;font-weight:400 700;font-display:swap;src:url({f}) format('woff2')}}"))) }
                }
                style { (PreEscaped(ERROR_STYLE)) }
            }
            body {
                main {
                    div.card {
                        img src="/logo.svg" alt="mau" width="112" height="112";
                        h1 { (title) }
                        p.lead { (message) }
                        p.help { "Go back to the app and try again, or tell its developer." }
                        p.links { a href="/" { "Go to mau" } }
                    }
                }
                // The same footer as every page (web/src/lib/components/Footer.svelte).
                footer {
                    a href="https://github.com/rri/mau" rel="noopener" { "github.com/rri/mau" }
                    span aria-hidden="true" { "·" }
                    span { "Source code licensed under AGPL-3.0" }
                }
            }
        }
    };
    let mut res = (status, m.into_string()).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

const ERROR_STYLE: &str = "
:root{color-scheme:light dark;--bg:#f5f7f6;--surface:#fff;--ink:#0d1512;--muted:#56635d;--line:#e1e7e4;--accent:#047857}
@media (prefers-color-scheme:dark){:root{--bg:#0a0f0d;--surface:#131a17;--ink:#e9f0ec;--muted:#95a59e;--line:#25302b;--accent:#34d399}}
*,*::before,*::after{box-sizing:border-box}
html,body{height:100%}
body{margin:0;display:flex;flex-direction:column;background:var(--bg);color:var(--ink);font:15px/1.5 'Instrument Sans',system-ui,sans-serif}
main{flex:1;display:grid;place-items:center;padding:24px 16px}
.card{max-width:420px;width:100%;background:var(--surface);border:1px solid var(--line);border-radius:18px;padding:28px 24px;display:grid;gap:12px;text-align:center}
img{margin:0 auto;border-radius:22.5%}h1{margin:0;font-size:24px;font-weight:600;letter-spacing:-0.01em;text-wrap:balance}
p{margin:0;overflow-wrap:anywhere}.lead{font-size:15px}.help{font-size:14px;color:var(--muted)}.links{font-size:15px}a{color:var(--accent)}
footer{display:flex;flex-wrap:wrap;justify-content:center;gap:4px 10px;padding:16px;font-size:13px;color:var(--muted)}footer a{color:inherit}";

/// Adds `params` to the app's redirect address, in the query or (with
/// `response_mode=fragment`) the fragment.
fn redirect_with(redirect_uri: &str, fragment: bool, params: &[(&str, &str)]) -> String {
    let Ok(mut u) = Url::parse(redirect_uri) else {
        return redirect_uri.to_string();
    };
    if fragment {
        let f = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params)
            .finish();
        u.set_fragment(Some(&f));
    } else {
        u.query_pairs_mut().extend_pairs(params);
    }
    u.to_string()
}

fn redirect_error(
    state: &AppState,
    redirect_uri: &str,
    fragment: bool,
    app_state: Option<&str>,
    error: &str,
    description: &str,
) -> Response {
    let iss = issuer(state);
    let mut params = vec![
        ("error", error),
        ("error_description", description),
        ("iss", &iss),
    ];
    if let Some(s) = app_state {
        params.push(("state", s));
    }
    Redirect::to(&redirect_with(redirect_uri, fragment, &params)).into_response()
}

/// Whether an app's address (any app at that origin) has been blocked.
async fn blocked(conn: &mut SqliteConnection, origin: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM oauth_clients WHERE origin = ? AND blocked = 1)",
    )
    .bind(origin)
    .fetch_one(conn)
    .await
}

fn is_b64url(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub async fn authorize(
    State(state): State<AppState>,
    ip: ClientIp,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    if state.limits.check("authorize", &ip.key(), 60).is_err() {
        return error_page(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests",
            "Wait a minute and try again.",
        );
    }
    // Problems with the app or where to send the answer: say so here.
    let client = match client::parse_client_id(q.client_id.as_deref().unwrap_or_default()) {
        Ok(c) => c,
        Err(msg) => {
            return error_page(
                StatusCode::BAD_REQUEST,
                "This sign-in link doesn't work",
                msg,
            );
        }
    };
    let Some(redirect_uri) = q.redirect_uri.clone() else {
        return error_page(
            StatusCode::BAD_REQUEST,
            "This sign-in link doesn't work",
            "It doesn't say where to send you back to (redirect_uri).",
        );
    };
    let host = host_of(&client.origin);
    // A blocked address is refused before anything is fetched from it.
    match state.db.acquire().await {
        Ok(mut conn) => match blocked(&mut conn, &client.origin).await {
            Ok(false) => {}
            Ok(true) => {
                return error_page(
                    StatusCode::FORBIDDEN,
                    "This app is blocked",
                    &format!(
                        "An admin of this mau has blocked {host}. It can't be connected to accounts here."
                    ),
                );
            }
            Err(e) => {
                tracing::error!("authorize: {e}");
                return error_page(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Something went wrong",
                    "Something went wrong on the server.",
                );
            }
        },
        Err(e) => {
            tracing::error!("authorize: {e}");
            return error_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                "Something went wrong on the server.",
            );
        }
    }
    let meta: Option<Metadata> = state.oauth_meta.get(&client).await;
    if !client::redirect_allowed(&client, meta.as_ref(), &redirect_uri) {
        return error_page(
            StatusCode::BAD_REQUEST,
            "This sign-in link doesn't work",
            &format!(
                "{host} can't send you back to that address (redirect_uri). An app may only use addresses on its own site, or ones its address lists."
            ),
        );
    }

    // From here the app can be told what's wrong.
    let fragment = match q.response_mode.as_deref() {
        None | Some("query") => false,
        Some("fragment") => true,
        Some(_) => {
            return redirect_error(
                &state,
                &redirect_uri,
                false,
                q.state.as_deref(),
                "invalid_request",
                "response_mode must be query or fragment.",
            );
        }
    };
    let fail = |error: &str, description: &str| {
        redirect_error(
            &state,
            &redirect_uri,
            fragment,
            q.state.as_deref(),
            error,
            description,
        )
    };
    if q.response_type.as_deref() != Some("code") {
        return fail("unsupported_response_type", "response_type must be code.");
    }
    let app_state = match q.state.as_deref() {
        Some(s) if !s.is_empty() && s.len() <= 500 => s.to_string(),
        _ => {
            return fail(
                "invalid_request",
                "state is required (at most 500 characters).",
            );
        }
    };
    if q.code_challenge_method.as_deref() != Some("S256") {
        return fail(
            "invalid_request",
            "PKCE is required: code_challenge_method=S256.",
        );
    }
    let challenge = q.code_challenge.clone().unwrap_or_default();
    if challenge.len() != 43 || !is_b64url(&challenge) {
        return fail(
            "invalid_request",
            "code_challenge must be the base64url SHA-256 of the code verifier (43 characters).",
        );
    }
    let scopes = match Scopes::parse(q.scope.as_deref().unwrap_or_default()) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return fail("invalid_scope", "Ask for at least one scope."),
        Err(unknown) => return fail("invalid_scope", &format!("Unknown scope: {unknown}.")),
    };
    let id = state.oauth.add_request(AuthRequest {
        client,
        name: meta.and_then(|m| m.name),
        redirect_uri,
        scopes,
        state: app_state,
        code_challenge: challenge,
        fragment,
    });
    Redirect::to(&format!("/authorize/{id}")).into_response()
}

// ---------------------------------------------------------------------------
// Step 2: the consent screen's API (signed-in browser, same origin)

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthRequestInfo {
    /// The app's address (its client_id).
    pub client_id: String,
    /// What the reader is shown: scheme://host[:port].
    pub origin: String,
    /// The host alone, e.g. stats.example.com.
    pub host: String,
    /// From the app's own metadata; shown beside the address, never instead.
    pub name: Option<String>,
    /// An app on the reader's own computer (localhost).
    pub local: bool,
    /// Where the browser goes back to, by origin (or the app's scheme).
    pub redirect_to: String,
    pub scopes: Vec<ScopeInfo>,
    /// Who would be connecting it.
    pub me: Me,
}

fn host_of(origin: &str) -> String {
    Url::parse(origin)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| u.port().map_or(h.to_string(), |p| format!("{h}:{p}")))
        })
        .unwrap_or_else(|| origin.to_string())
}

fn redirect_shown(uri: &str) -> String {
    match Url::parse(uri) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            host_of(&u.origin().ascii_serialization())
        }
        Ok(u) => format!("{}:", u.scheme()),
        Err(_) => uri.to_string(),
    }
}

fn expired() -> ApiError {
    ApiError::not_found(
        "This request has expired or was already answered. Go back to the app and start again.",
    )
}

pub async fn request_info(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<AuthRequestInfo>> {
    let r = state.oauth.request(&id).ok_or_else(expired)?;
    Ok(Json(AuthRequestInfo {
        client_id: r.client.id.clone(),
        host: host_of(&r.client.origin),
        origin: r.client.origin.clone(),
        name: r.name.clone(),
        local: r.client.local,
        redirect_to: redirect_shown(&r.redirect_uri),
        scopes: r.scopes.infos(),
        me: Me::from(&s.user),
    }))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthAnswer {
    /// Send the browser here (back to the app).
    pub redirect: String,
}

/// Allows the app everything it asked for. Needs a password confirmation
/// from the last 10 minutes: a connection outlives this session.
pub async fn approve(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<AuthAnswer>> {
    state.oauth.request(&id).ok_or_else(expired)?;
    s.require_recent_confirmation()?;
    let r = state.oauth.take_request(&id).ok_or_else(expired)?;
    let mut tx = state.db.begin().await?;
    if blocked(&mut tx, &r.client.origin).await? {
        return Err(ApiError::forbidden(format!(
            "An admin of this mau has blocked {}.",
            r.client.origin
        )));
    }
    let now = now_ms();
    sqlx::query(
        "INSERT INTO oauth_clients (id, origin, name, first_seen) VALUES (?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = COALESCE(excluded.name, name)",
    )
    .bind(&r.client.id)
    .bind(&r.client.origin)
    .bind(&r.name)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT id, scopes FROM oauth_grants WHERE user_id = ? AND client_id = ?")
            .bind(&s.user.id)
            .bind(&r.client.id)
            .fetch_optional(&mut *tx)
            .await?;
    let grant_id = match existing {
        Some((gid, scopes)) => {
            let all = Scopes::parse(&scopes).unwrap_or_default().union(&r.scopes);
            sqlx::query("UPDATE oauth_grants SET scopes = ? WHERE id = ?")
                .bind(all.to_string())
                .bind(&gid)
                .execute(&mut *tx)
                .await?;
            gid
        }
        None => {
            let gid = new_id();
            sqlx::query(
                "INSERT INTO oauth_grants (id, user_id, client_id, scopes, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&gid)
            .bind(&s.user.id)
            .bind(&r.client.id)
            .bind(r.scopes.to_string())
            .bind(now)
            .execute(&mut *tx)
            .await?;
            gid
        }
    };
    tx.commit().await?;
    let code = state.oauth.add_code(AuthCode {
        user_id: s.user.id.clone(),
        grant_id,
        client_id: r.client.id.clone(),
        redirect_uri: r.redirect_uri.clone(),
        code_challenge: r.code_challenge.clone(),
        scopes: r.scopes.clone(),
        expires: now + CODE_TTL_MS,
        used: false,
    });
    let iss = issuer(&state);
    Ok(Json(AuthAnswer {
        redirect: redirect_with(
            &r.redirect_uri,
            r.fragment,
            &[("code", &code), ("state", &r.state), ("iss", &iss)],
        ),
    }))
}

pub async fn deny(
    State(state): State<AppState>,
    Authed(_): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<AuthAnswer>> {
    let r = state.oauth.take_request(&id).ok_or_else(expired)?;
    let iss = issuer(&state);
    Ok(Json(AuthAnswer {
        redirect: redirect_with(
            &r.redirect_uri,
            r.fragment,
            &[
                ("error", "access_denied"),
                ("error_description", "The person didn't allow it."),
                ("state", &r.state),
                ("iss", &iss),
            ],
        ),
    }))
}

// ---------------------------------------------------------------------------
// Step 3: POST /oauth/token, /oauth/revoke (any origin, no cookies)

/// An OAuth error answer (RFC 6749 §5.2).
pub struct OAuthError {
    status: StatusCode,
    error: &'static str,
    description: String,
}

impl OAuthError {
    fn new(error: &'static str, description: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error,
            description: description.into(),
        }
    }
}

impl From<sqlx::Error> for OAuthError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("oauth: {e}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "server_error",
            description: "Something went wrong on the server.".into(),
        }
    }
}

impl From<ApiError> for OAuthError {
    fn from(e: ApiError) -> Self {
        Self {
            status: e.status,
            error: if e.status == StatusCode::TOO_MANY_REQUESTS {
                "slow_down"
            } else {
                "server_error"
            },
            description: e.message,
        }
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let mut res = (
            self.status,
            Json(serde_json::json!({
                "error": self.error,
                "error_description": self.description,
            })),
        )
            .into_response();
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        res
    }
}

#[derive(Serialize)]
pub struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    refresh_token: String,
    scope: String,
}

fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let ok_chars = verifier
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b));
    (43..=128).contains(&verifier.len())
        && ok_chars
        && URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}

/// Issues an access and a refresh token for a grant, and clears the grant's
/// expired tokens.
async fn issue(
    conn: &mut SqliteConnection,
    user_id: &str,
    grant_id: &str,
    scopes: &Scopes,
) -> sqlx::Result<TokenResponse> {
    let now = now_ms();
    sqlx::query("DELETE FROM api_tokens WHERE grant_id = ? AND expires_at <= ?")
        .bind(grant_id)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    // An app that refreshes very often mustn't pile up rows: keep its newest
    // live access tokens, and the newest spent refresh tokens (kept to spot
    // reuse).
    for (kind, keep, cond) in [
        // One access token is about to be added.
        ("access", MAX_ACCESS_PER_GRANT - 1, "1"),
        ("refresh", MAX_SPENT_PER_GRANT, "used_at IS NOT NULL"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM api_tokens WHERE grant_id = ?1 AND kind = ?2 AND {cond} AND id NOT IN (
                 SELECT id FROM api_tokens WHERE grant_id = ?1 AND kind = ?2 AND {cond}
                 ORDER BY created_at DESC LIMIT ?3)"
        )))
        .bind(grant_id)
        .bind(kind)
        .bind(keep)
        .execute(&mut *conn)
        .await?;
    }
    let (_, access) = token::insert(
        &mut *conn,
        NewToken {
            user_id,
            kind: Kind::Access,
            grant_id: Some(grant_id),
            name: "",
            scopes,
            expires_at: Some(now + ACCESS_LIFETIME_MS),
        },
    )
    .await?;
    let (_, refresh) = token::insert(
        &mut *conn,
        NewToken {
            user_id,
            kind: Kind::Refresh,
            grant_id: Some(grant_id),
            name: "",
            scopes,
            expires_at: Some(now + REFRESH_LIFETIME_MS),
        },
    )
    .await?;
    Ok(TokenResponse {
        access_token: access,
        token_type: "Bearer",
        expires_in: ACCESS_LIFETIME_MS / 1000,
        refresh_token: refresh,
        scope: scopes.to_string(),
    })
}

/// Ends an app's access for one person: the grant and all its tokens.
async fn end_grant(conn: &mut SqliteConnection, grant_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM oauth_grants WHERE id = ?")
        .bind(grant_id)
        .execute(conn)
        .await?;
    Ok(())
}

fn field<'a>(f: &'a HashMap<String, String>, name: &str) -> Result<&'a str, OAuthError> {
    f.get(name)
        .map(String::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| OAuthError::new("invalid_request", format!("{name} is required.")))
}

pub async fn token(
    State(state): State<AppState>,
    ip: ClientIp,
    Form(f): Form<HashMap<String, String>>,
) -> Result<Response, OAuthError> {
    state.limits.check("token", &ip.key(), 60)?;
    if f.contains_key("client_secret") {
        return Err(OAuthError {
            status: StatusCode::UNAUTHORIZED,
            error: "invalid_client",
            description: "mau's apps have no secrets: send client_id and the PKCE code_verifier."
                .into(),
        });
    }
    let client_id = field(&f, "client_id")?;
    let out = match field(&f, "grant_type")? {
        "authorization_code" => {
            let code = field(&f, "code")?;
            let redirect_uri = field(&f, "redirect_uri")?;
            let verifier = field(&f, "code_verifier")?;
            let bad = || {
                OAuthError::new(
                    "invalid_grant",
                    "The code isn't valid. It may have expired or been used.",
                )
            };
            let c = state.oauth.use_code(code).ok_or_else(bad)?;
            if c.used {
                // A code used twice: someone else may have it. End the access
                // it led to (RFC 6749 §10.5).
                let mut conn = state.db.acquire().await?;
                end_grant(&mut conn, &c.grant_id).await?;
                return Err(bad());
            }
            if c.client_id != client_id || c.redirect_uri != redirect_uri {
                return Err(bad());
            }
            if !pkce_matches(verifier, &c.code_challenge) {
                return Err(OAuthError::new(
                    "invalid_grant",
                    "The code_verifier doesn't match the code_challenge.",
                ));
            }
            let mut tx = state.db.begin().await?;
            let live: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM oauth_grants g JOIN users u ON u.id = g.user_id
                   JOIN oauth_clients c ON c.id = g.client_id
                 WHERE g.id = ? AND g.user_id = ? AND u.suspended = 0 AND c.blocked = 0)",
            )
            .bind(&c.grant_id)
            .bind(&c.user_id)
            .fetch_one(&mut *tx)
            .await?;
            if !live {
                return Err(bad());
            }
            let out = issue(&mut tx, &c.user_id, &c.grant_id, &c.scopes).await?;
            tx.commit().await?;
            out
        }
        "refresh_token" => {
            let secret = field(&f, "refresh_token")?;
            let bad = || {
                OAuthError::new(
                    "invalid_grant",
                    "The refresh token isn't valid. Sign in again.",
                )
            };
            let mut tx = state.db.begin().await?;
            let row: Option<(String, String, String, String, Option<i64>, Option<i64>)> = sqlx::query_as(
                "SELECT t.id, t.user_id, t.grant_id, t.scopes, t.expires_at, t.used_at FROM api_tokens t
                 WHERE t.token_hash = ? AND t.kind = 'refresh'",
            )
            .bind(sha256(secret))
            .fetch_optional(&mut *tx)
            .await?;
            let Some((id, user_id, grant_id, scopes, expires_at, used_at)) = row else {
                return Err(bad());
            };
            let (owner, suspended, blocked): (String, bool, bool) = sqlx::query_as(
                "SELECT g.client_id, u.suspended, c.blocked FROM oauth_grants g
                   JOIN users u ON u.id = g.user_id JOIN oauth_clients c ON c.id = g.client_id
                 WHERE g.id = ?",
            )
            .bind(&grant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(bad)?;
            if owner != client_id {
                return Err(bad());
            }
            if used_at.is_some() {
                // A refresh token used twice: it was copied. End the access.
                end_grant(&mut tx, &grant_id).await?;
                tx.commit().await?;
                return Err(bad());
            }
            if suspended || blocked || expires_at.is_some_and(|e| e <= now_ms()) {
                return Err(bad());
            }
            let granted = Scopes::parse(&scopes).unwrap_or_default();
            let scopes = match f.get("scope").filter(|s| !s.trim().is_empty()) {
                Some(s) => {
                    let asked = Scopes::parse(s).map_err(|u| {
                        OAuthError::new("invalid_scope", format!("Unknown scope: {u}."))
                    })?;
                    if !asked.is_subset(&granted) {
                        return Err(OAuthError::new(
                            "invalid_scope",
                            "A refresh can only ask for scopes already allowed.",
                        ));
                    }
                    asked
                }
                None => granted,
            };
            let n =
                sqlx::query("UPDATE api_tokens SET used_at = ? WHERE id = ? AND used_at IS NULL")
                    .bind(now_ms())
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
            if n != 1 {
                return Err(bad());
            }
            let out = issue(&mut tx, &user_id, &grant_id, &scopes).await?;
            tx.commit().await?;
            out
        }
        _ => {
            return Err(OAuthError::new(
                "unsupported_grant_type",
                "grant_type must be authorization_code or refresh_token.",
            ));
        }
    };
    let mut res = Json(out).into_response();
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(res)
}

/// Revokes a token (RFC 7009). Always answers 200, whether or not the token
/// was known. A refresh token ends the app's access for that person; an
/// access token ends only itself; a personal token, presented here, ends
/// itself (so a leaked one can be killed by whoever finds it).
pub async fn revoke(
    State(state): State<AppState>,
    ip: ClientIp,
    Form(f): Form<HashMap<String, String>>,
) -> Result<Response, OAuthError> {
    state.limits.check("token", &ip.key(), 60)?;
    let secret = field(&f, "token")?;
    let mut tx = state.db.begin().await?;
    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, kind, grant_id FROM api_tokens WHERE token_hash = ?")
            .bind(sha256(secret))
            .fetch_optional(&mut *tx)
            .await?;
    if let Some((id, kind, grant_id)) = row {
        let app: Option<String> = match &grant_id {
            Some(g) => {
                sqlx::query_scalar("SELECT client_id FROM oauth_grants WHERE id = ?")
                    .bind(g)
                    .fetch_optional(&mut *tx)
                    .await?
            }
            None => None,
        };
        // An app's tokens are revoked only by that app.
        let ours = app.is_none() || f.get("client_id") == app.as_ref();
        match (kind.as_str(), grant_id) {
            ("refresh", Some(g)) if ours => end_grant(&mut tx, &g).await?,
            _ if ours => {
                sqlx::query("DELETE FROM api_tokens WHERE id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
            _ => {}
        }
    }
    tx.commit().await?;
    let mut res = Json(serde_json::json!({})).into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

/// `GET /.well-known/oauth-authorization-server` (RFC 8414).
pub async fn metadata(State(state): State<AppState>) -> Response {
    let iss = issuer(&state);
    let mut res = Json(serde_json::json!({
        "issuer": iss,
        "authorization_endpoint": format!("{iss}/oauth/authorize"),
        "token_endpoint": format!("{iss}/oauth/token"),
        "revocation_endpoint": format!("{iss}/oauth/revoke"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query", "fragment"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": super::scope::ALL.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        "authorization_response_iss_parameter_supported": true,
        "client_id_metadata_document_supported": true,
        "service_documentation": "https://github.com/rri/mau/blob/main/docs/API.md",
    }))
    .into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    res
}
