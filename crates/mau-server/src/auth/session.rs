//! Sessions: the cookie, lookups, and request extractors.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use sqlx::SqlitePool;

use super::User;
use crate::app::AppState;
use crate::error::ApiError;
use crate::util::{DAY_MS, HOUR_MS, MINUTE_MS, now_ms, random_token, sha256, short_id};

pub const COOKIE: &str = "mau_session";
const LIFETIME_MS: i64 = 30 * DAY_MS;
/// Sensitive actions need a passkey confirmation at most this old.
pub const REAUTH_WINDOW_MS: i64 = 5 * MINUTE_MS;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user: User,
    /// Signed in with a recovery code or enrollment link: may only add a passkey.
    pub restricted: bool,
    pub reauth_at: Option<i64>,
}

impl Session {
    pub fn require_recent_passkey(&self) -> Result<(), ApiError> {
        match self.reauth_at {
            Some(at) if now_ms() - at <= REAUTH_WINDOW_MS => Ok(()),
            _ => Err(ApiError::reauth_required()),
        }
    }
}

/// Creates a session and returns the cookie value.
pub async fn create(
    db: &SqlitePool,
    user_id: &str,
    device: &str,
    restricted: bool,
    reauthed: bool,
) -> sqlx::Result<String> {
    let token = random_token();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO sessions (id, token_hash, user_id, device, created_at, last_seen_at, expires_at, restricted, reauth_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(short_id())
    .bind(sha256(&token))
    .bind(user_id)
    .bind(device)
    .bind(now)
    .bind(now)
    .bind(now + LIFETIME_MS)
    .bind(restricted)
    .bind(reauthed.then_some(now))
    .execute(db)
    .await?;
    Ok(token)
}

pub fn cookie_header(token: &str, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{secure}",
        LIFETIME_MS / 1000
    ))
    .expect("cookie is ASCII")
}

pub fn clear_cookie_header(secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}"))
        .expect("cookie is ASCII")
}

pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

/// Looks up a live session by cookie value and extends it if due.
pub async fn lookup(db: &SqlitePool, token: &str) -> sqlx::Result<Option<Session>> {
    let now = now_ms();
    let row: Option<(String, bool, Option<i64>, i64)> = sqlx::query_as(
        "SELECT id, restricted, reauth_at, last_seen_at FROM sessions WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(sha256(token))
    .bind(now)
    .fetch_optional(db)
    .await?;
    let Some((id, restricted, reauth_at, last_seen)) = row else { return Ok(None) };
    let user: Option<User> = sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE id = (SELECT user_id FROM sessions WHERE id = ?)"
    ))
    .bind(&id)
    .fetch_optional(db)
    .await?;
    let Some(user) = user else { return Ok(None) };
    if user.suspended {
        return Ok(None);
    }
    if now - last_seen > HOUR_MS {
        sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?")
            .bind(now)
            .bind(now + LIFETIME_MS)
            .bind(&id)
            .execute(db)
            .await?;
    }
    Ok(Some(Session { id, user, restricted, reauth_at }))
}

/// Any signed-in session, including restricted ones.
pub struct AnySession(pub Session);

/// A full (unrestricted) session.
pub struct Authed(pub Session);

/// A full session belonging to an admin.
pub struct Admin(pub Session);

impl FromRequestParts<AppState> for AnySession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        if let Some(s) = parts.extensions.get::<Session>() {
            return Ok(AnySession(s.clone()));
        }
        let token = token_from_headers(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        let session = lookup(&state.db, &token).await?.ok_or_else(ApiError::unauthorized)?;
        parts.extensions.insert(session.clone());
        Ok(AnySession(session))
    }
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let AnySession(s) = AnySession::from_request_parts(parts, state).await?;
        if s.restricted {
            return Err(ApiError::forbidden("Add a passkey before continuing.").with_code("enroll_required"));
        }
        Ok(Authed(s))
    }
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Authed(s) = Authed::from_request_parts(parts, state).await?;
        if !s.user.is_admin {
            return Err(ApiError::forbidden("Only admins can do that."));
        }
        Ok(Admin(s))
    }
}

impl ApiError {
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }
}
