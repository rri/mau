//! Bearer tokens: personal tokens people make in Settings, and the access
//! and refresh tokens apps get through OAuth. Like session cookies, they're
//! 32 random bytes and only their SHA-256 is stored.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use sqlx::SqliteConnection;

use super::scope::Scopes;
use crate::app::AppState;
use crate::auth::User;
use crate::auth::session::Session;
use crate::error::ApiError;
use crate::util::{HOUR_MS, now_ms, random_token, sha256, short_id};

/// An app's access token lasts an hour; its refresh token 30 days from its
/// last use.
pub const ACCESS_LIFETIME_MS: i64 = HOUR_MS;
pub const REFRESH_LIFETIME_MS: i64 = 30 * 24 * HOUR_MS;
/// Requests per minute per token.
pub const PER_MINUTE: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Personal,
    Access,
    Refresh,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Personal => "personal",
            Kind::Access => "access",
            Kind::Refresh => "refresh",
        }
    }
    /// Recognizable prefixes, so a leaked token can be spotted (and secret
    /// scanners can match them).
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Personal => "mau_pat_",
            Kind::Access => "mau_at_",
            Kind::Refresh => "mau_rt_",
        }
    }
}

pub struct NewToken<'a> {
    pub user_id: &'a str,
    pub kind: Kind,
    pub grant_id: Option<&'a str>,
    pub name: &'a str,
    pub scopes: &'a Scopes,
    pub expires_at: Option<i64>,
}

/// Stores a new token and returns its ID and the secret (shown once).
pub async fn insert(
    conn: &mut SqliteConnection,
    t: NewToken<'_>,
) -> sqlx::Result<(String, String)> {
    let secret = format!("{}{}", t.kind.prefix(), random_token());
    let id = short_id();
    let hint: String = secret.chars().take(t.kind.prefix().len() + 4).collect();
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, kind, grant_id, name, token_hash, hint, scopes, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(t.user_id)
    .bind(t.kind.as_str())
    .bind(t.grant_id)
    .bind(t.name)
    .bind(sha256(&secret))
    .bind(hint)
    .bind(t.scopes.to_string())
    .bind(now_ms())
    .bind(t.expires_at)
    .execute(conn)
    .await?;
    Ok((id, secret))
}

/// Ends every token and app connection of a user: after recovering an
/// account, nothing the previous holder made should keep working.
pub async fn revoke_all(conn: &mut SqliteConnection, user_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM api_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM oauth_grants WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A request made with a valid bearer token.
#[derive(Debug, Clone)]
pub struct TokenAuth {
    pub token_id: String,
    pub user: User,
    pub scopes: Scopes,
}

impl TokenAuth {
    /// Refuses unless the token has `scope`.
    pub fn require(&self, scope: &'static str) -> Result<(), ApiError> {
        if self.scopes.allows(scope) {
            Ok(())
        } else {
            Err(insufficient_scope(scope))
        }
    }

    pub fn allows(&self, scope: &str) -> bool {
        self.scopes.allows(scope)
    }

    /// The token's user as a session that has never confirmed a password, for
    /// reusing the app's own handlers. Anything that needs a confirmation
    /// refuses it.
    pub fn session(&self) -> Session {
        Session {
            id: String::new(),
            user: self.user.clone(),
            reauth_at: None,
        }
    }
}

pub fn insufficient_scope(scope: &str) -> ApiError {
    let mut e = ApiError::new(
        StatusCode::FORBIDDEN,
        "insufficient_scope",
        format!("This token doesn't allow that. It needs the {scope} scope."),
    );
    e.www_authenticate = Some(format!(
        "Bearer realm=\"mau\", error=\"insufficient_scope\", scope=\"{scope}\""
    ));
    e
}

fn invalid_token(message: &str) -> ApiError {
    let mut e = ApiError::new(StatusCode::UNAUTHORIZED, "invalid_token", message);
    e.www_authenticate = Some("Bearer realm=\"mau\", error=\"invalid_token\"".into());
    e
}

fn no_token() -> ApiError {
    let mut e = ApiError::new(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Send a token: Authorization: Bearer <token>.",
    );
    e.www_authenticate = Some("Bearer realm=\"mau\"".into());
    e
}

/// The token in `Authorization: Bearer …`. Cookies are never looked at.
fn bearer(parts: &Parts) -> Option<&str> {
    let v = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.trim().split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// Looks up a usable token: personal or access (never refresh), unexpired,
/// of a user who isn't suspended, for an app that isn't blocked.
pub async fn lookup(state: &AppState, secret: &str) -> Result<Option<TokenAuth>, ApiError> {
    if !secret.starts_with(Kind::Personal.prefix()) && !secret.starts_with(Kind::Access.prefix()) {
        return Ok(None);
    }
    let now = now_ms();
    let row: Option<(String, String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT t.id, t.user_id, t.scopes, t.grant_id, t.last_used_at FROM api_tokens t
         WHERE t.token_hash = ? AND t.kind IN ('personal', 'access')
           AND (t.expires_at IS NULL OR t.expires_at > ?)
           AND (t.grant_id IS NULL OR NOT EXISTS (
               SELECT 1 FROM oauth_grants g JOIN oauth_clients c ON c.id = g.client_id
               WHERE g.id = t.grant_id AND c.blocked = 1))",
    )
    .bind(sha256(secret))
    .bind(now)
    .fetch_optional(&state.db)
    .await?;
    let Some((id, user_id, scopes, grant_id, last_used)) = row else {
        return Ok(None);
    };
    let user: Option<User> = sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE id = ?"
    ))
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;
    let Some(user) = user.filter(|u| !u.suspended) else {
        return Ok(None);
    };
    if last_used.is_none_or(|t| now - t > HOUR_MS) {
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(&id)
            .execute(&state.db)
            .await?;
        if let Some(g) = &grant_id {
            sqlx::query("UPDATE oauth_grants SET last_used_at = ? WHERE id = ?")
                .bind(now)
                .bind(g)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(Some(TokenAuth {
        token_id: id,
        user,
        // Stored scopes were checked when the token was made; any that are
        // no longer known are dropped rather than refusing the token.
        scopes: Scopes::from_iter_strs(
            scopes
                .split_ascii_whitespace()
                .filter(|s| super::scope::ALL.iter().any(|(k, _)| k == s)),
        )
        .unwrap_or_default(),
    }))
}

impl FromRequestParts<AppState> for TokenAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        if let Some(t) = parts.extensions.get::<TokenAuth>() {
            return Ok(t.clone());
        }
        let secret = bearer(parts).ok_or_else(no_token)?;
        let auth = lookup(state, secret).await?.ok_or_else(|| {
            invalid_token("This token isn't valid. It may have expired or been revoked.")
        })?;
        state.limits.check("api", &auth.token_id, PER_MINUTE)?;
        parts.extensions.insert(auth.clone());
        Ok(auth)
    }
}

/// Deletes expired tokens every hour (they no longer work anyway).
pub fn schedule_cleanup(db: sqlx::SqlitePool) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = sqlx::query("DELETE FROM api_tokens WHERE expires_at <= ?")
                .bind(now_ms())
                .execute(&db)
                .await
            {
                tracing::warn!("clearing expired tokens: {e}");
            }
        }
    });
}
