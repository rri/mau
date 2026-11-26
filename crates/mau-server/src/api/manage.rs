//! Settings → Access Tokens (each person's own), and Admin → Apps.
//! These are for the signed-in browser only: no token can reach them, so a
//! token can never make more tokens or connect more apps.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::scope::{ScopeInfo, Scopes, all_infos};
use super::token::{self, Kind, NewToken};
use crate::app::AppState;
use crate::audit;
use crate::auth::handlers::{Ok_, ok};
use crate::auth::session::{Admin, Authed};
use crate::error::{ApiError, ApiResult};
use crate::util::{DAY_MS, now_ms};

/// Personal tokens a person may have at once.
const MAX_TOKENS: i64 = 50;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    /// The token's first characters, e.g. mau_pat_3fA9.
    pub hint: String,
    pub scopes: Vec<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    /// Null: never expires.
    #[ts(type = "number | null")]
    pub expires_at: Option<i64>,
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ConnectedApp {
    /// The connection's ID (for revoking).
    pub id: String,
    pub client_id: String,
    pub origin: String,
    pub host: String,
    pub name: Option<String>,
    pub scopes: Vec<ScopeInfo>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ApiAccess {
    pub apps: Vec<ConnectedApp>,
    pub tokens: Vec<TokenInfo>,
    /// Every scope, for making a token.
    pub scopes: Vec<ScopeInfo>,
}

fn host(origin: &str) -> String {
    origin
        .split_once("://")
        .map_or(origin, |(_, h)| h)
        .to_string()
}

type TokenTuple = (
    String,
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<i64>,
);

fn token_info(
    (id, name, hint, scopes, created_at, expires_at, last_used_at): TokenTuple,
) -> TokenInfo {
    TokenInfo {
        id,
        name,
        hint,
        scopes: Scopes::from_iter_strs(
            scopes
                .split_ascii_whitespace()
                .filter(|s| super::scope::ALL.iter().any(|(k, _)| k == s)),
        )
        .unwrap_or_default()
        .list(),
        created_at,
        expires_at,
        last_used_at,
    }
}

pub async fn list(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<ApiAccess>> {
    let now = now_ms();
    let tokens: Vec<TokenTuple> = sqlx::query_as(
        "SELECT id, name, hint, scopes, created_at, expires_at, last_used_at FROM api_tokens
         WHERE user_id = ? AND kind = 'personal' AND (expires_at IS NULL OR expires_at > ?)
         ORDER BY created_at DESC",
    )
    .bind(&s.user.id)
    .bind(now)
    .fetch_all(&state.db)
    .await?;
    let apps: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT g.id, c.id, c.origin, c.name, g.scopes, g.created_at, g.last_used_at
             FROM oauth_grants g JOIN oauth_clients c ON c.id = g.client_id
             WHERE g.user_id = ? ORDER BY g.created_at DESC",
    )
    .bind(&s.user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiAccess {
        tokens: tokens.into_iter().map(token_info).collect(),
        apps: apps
            .into_iter()
            .map(
                |(id, client_id, origin, name, scopes, created_at, last_used_at)| ConnectedApp {
                    id,
                    client_id,
                    host: host(&origin),
                    origin,
                    name,
                    scopes: Scopes::from_iter_strs(
                        scopes
                            .split_ascii_whitespace()
                            .filter(|s| super::scope::ALL.iter().any(|(k, _)| k == s)),
                    )
                    .unwrap_or_default()
                    .infos(),
                    created_at,
                    last_used_at,
                },
            )
            .collect(),
        scopes: all_infos(),
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct NewApiToken {
    pub name: String,
    pub scopes: Vec<String>,
    /// 30, 90 or 365; null for a token that never expires.
    #[ts(type = "number | null")]
    pub expires_in_days: Option<i64>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CreatedApiToken {
    pub token: TokenInfo,
    /// The token itself, shown this once.
    pub secret: String,
}

/// Makes a personal token. Needs a password confirmation from the last 10
/// minutes: a token outlives this session.
pub async fn create(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<NewApiToken>,
) -> ApiResult<Json<CreatedApiToken>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(ApiError::bad_request(
            "name",
            "Give the token a name of up to 60 characters, so you know what uses it.",
        ));
    }
    let scopes = Scopes::from_iter_strs(req.scopes.iter().map(String::as_str))
        .map_err(|u| ApiError::bad_request("scopes", format!("Unknown scope: {u}.")))?;
    if scopes.is_empty() {
        return Err(ApiError::bad_request(
            "scopes",
            "Choose at least one thing the token may do.",
        ));
    }
    let expires_at = match req.expires_in_days {
        None => None,
        Some(d @ (30 | 90 | 365)) => Some(now_ms() + d * DAY_MS),
        Some(_) => {
            return Err(ApiError::bad_request(
                "expires",
                "Tokens last 30, 90 or 365 days, or never expire.",
            ));
        }
    };
    s.require_recent_confirmation()?;
    let mut tx = state.db.begin().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_tokens WHERE user_id = ? AND kind = 'personal' AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(&s.user.id)
    .bind(now_ms())
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_TOKENS {
        return Err(ApiError::conflict(
            "too_many",
            format!("You have {MAX_TOKENS} tokens. Revoke one you don't use first."),
        ));
    }
    let (id, secret) = token::insert(
        &mut tx,
        NewToken {
            user_id: &s.user.id,
            kind: Kind::Personal,
            grant_id: None,
            name,
            scopes: &scopes,
            expires_at,
        },
    )
    .await?;

    let row: TokenTuple = sqlx::query_as(
        "SELECT id, name, hint, scopes, created_at, expires_at, last_used_at FROM api_tokens WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(CreatedApiToken {
        token: token_info(row),
        secret,
    }))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    sqlx::query("DELETE FROM api_tokens WHERE id = ? AND user_id = ? AND kind = 'personal'")
        .bind(&id)
        .bind(&s.user.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

/// Ends an app's access: the connection and all its tokens.
pub async fn revoke_app(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    sqlx::query("DELETE FROM oauth_grants WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&s.user.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

// ---------------------------------------------------------------------------
// Admin → Apps

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AppRow {
    pub client_id: String,
    pub origin: String,
    pub host: String,
    pub name: Option<String>,
    /// How many people have it connected (never who).
    #[ts(type = "number")]
    pub people: i64,
    #[ts(type = "number")]
    pub first_seen: i64,
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
    pub blocked: bool,
}

pub async fn admin_list(State(state): State<AppState>, _: Admin) -> ApiResult<Json<Vec<AppRow>>> {
    let rows: Vec<(String, String, Option<String>, i64, i64, Option<i64>, bool)> = sqlx::query_as(
        "SELECT c.id, c.origin, c.name, c.first_seen,
                (SELECT COUNT(*) FROM oauth_grants g WHERE g.client_id = c.id),
                (SELECT MAX(g.last_used_at) FROM oauth_grants g WHERE g.client_id = c.id),
                c.blocked
         FROM oauth_clients c ORDER BY c.blocked, c.origin, c.id",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(client_id, origin, name, first_seen, people, last_used_at, blocked)| AppRow {
                    client_id,
                    host: host(&origin),
                    origin,
                    name,
                    people,
                    first_seen,
                    last_used_at,
                    blocked,
                },
            )
            .collect(),
    ))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct AppBlock {
    pub client_id: String,
    pub blocked: bool,
}

/// Blocks (or unblocks) every app at an address. Blocking ends everyone's
/// connections to it; unblocking lets people connect it again.
pub async fn admin_block(
    State(state): State<AppState>,
    a: Admin,
    Json(req): Json<AppBlock>,
) -> ApiResult<Json<Ok_>> {
    let mut tx = state.db.begin().await?;
    let origin: String = sqlx::query_scalar("SELECT origin FROM oauth_clients WHERE id = ?")
        .bind(&req.client_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("No such app."))?;
    sqlx::query("UPDATE oauth_clients SET blocked = ? WHERE origin = ?")
        .bind(req.blocked)
        .bind(&origin)
        .execute(&mut *tx)
        .await?;
    if req.blocked {
        sqlx::query(
            "DELETE FROM oauth_grants WHERE client_id IN (SELECT id FROM oauth_clients WHERE origin = ?)",
        )
        .bind(&origin)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(
        &mut *tx,
        Some((&a.0.user.id, &a.0.user.username)),
        format!(
            "{} the app {origin}",
            if req.blocked { "blocked" } else { "unblocked" }
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}
