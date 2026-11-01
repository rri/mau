//! Account settings: profile, passkeys, sessions, recovery codes, deletion.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::handlers::{Me, Ok_, clean_display_name, clean_time_zone, is_secure, ok, username_taken};
use super::recovery;
use super::session::{self, Authed};
use crate::app::AppState;
use crate::audit;
use crate::error::{ApiError, ApiResult};
use crate::util::{DAY_MS, now_ms};

// ---------------------------------------------------------------------------
// Profile

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct AccountUpdate {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub time_zone: Option<String>,
    pub username: Option<String>,
}

pub async fn update_account(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<AccountUpdate>,
) -> ApiResult<Json<Me>> {
    let mut user = s.user.clone();
    if let Some(d) = &req.display_name {
        user.display_name = clean_display_name(d)?;
    }
    if let Some(b) = &req.bio {
        let b = b.trim();
        if b.chars().count() > 280 {
            return Err(ApiError::bad_request("bio", "Keep the bio to 280 characters."));
        }
        user.bio = b.to_string();
    }
    if let Some(tz) = &req.time_zone {
        user.time_zone = clean_time_zone(tz)?;
    }
    let mut tx = state.db.begin().await?;
    if let Some(u) = &req.username {
        let name = mau_core::username::validate(u).map_err(|e| ApiError::bad_request("username", e.message()))?;
        if name != user.username {
            s.require_recent_passkey()?;
            if username_taken(&mut *tx, &name, Some(&user.id)).await? {
                return Err(ApiError::conflict("username_taken", format!("@{name} is taken.")));
            }
            sqlx::query("DELETE FROM username_reservations WHERE username = ?").bind(&name).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO username_reservations (username, user_id, until) VALUES (?, ?, ?)
                 ON CONFLICT(username) DO UPDATE SET user_id = excluded.user_id, until = excluded.until",
            )
            .bind(&user.username)
            .bind(&user.id)
            .bind(now_ms() + 30 * DAY_MS)
            .execute(&mut *tx)
            .await?;
            user.username = name;
        }
    }
    sqlx::query("UPDATE users SET display_name = ?, bio = ?, time_zone = ?, username = ? WHERE id = ?")
        .bind(&user.display_name)
        .bind(&user.bio)
        .bind(&user.time_zone)
        .bind(&user.username)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(Me::from(&user)))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct AccountDelete {
    pub username: String,
}

pub async fn delete_account(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<AccountDelete>,
) -> ApiResult<Response> {
    s.require_recent_passkey()?;
    if req.username.trim().to_ascii_lowercase() != s.user.username {
        return Err(ApiError::bad_request("username", "The username didn't match. Nothing was deleted."));
    }
    let mut tx = state.db.begin().await?;
    if s.user.is_admin {
        let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND suspended = 0")
            .fetch_one(&mut *tx)
            .await?;
        if admins <= 1 {
            return Err(ApiError::forbidden(
                "You're the only admin. Make someone else an admin before deleting your account.",
            ));
        }
    }
    crate::library::delete_user_data(&mut tx, &s.user.id).await?;
    sqlx::query("DELETE FROM users WHERE id = ?").bind(&s.user.id).execute(&mut *tx).await?;
    audit::record(&mut *tx, None, format!("{} deleted their account", s.user.username)).await?;
    tx.commit().await?;
    Ok((
        AppendHeaders([(header::SET_COOKIE, session::clear_cookie_header(is_secure(&state)))]),
        Json(Ok_ { ok: true }),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Passkeys

#[derive(Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

pub async fn list_passkeys(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<Vec<PasskeyInfo>>> {
    let rows = sqlx::query_as("SELECT id, name, created_at, last_used_at FROM passkeys WHERE user_id = ? ORDER BY created_at")
        .bind(&s.user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct PasskeyRename {
    pub name: String,
}

pub async fn rename_passkey(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    Json(req): Json<PasskeyRename>,
) -> ApiResult<Json<Ok_>> {
    let name: String = req.name.trim().chars().take(60).collect();
    if name.is_empty() {
        return Err(ApiError::bad_request("name", "Enter a name."));
    }
    let n = sqlx::query("UPDATE passkeys SET name = ? WHERE id = ? AND user_id = ?")
        .bind(name)
        .bind(id)
        .bind(&s.user.id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("No such passkey."));
    }
    Ok(ok())
}

pub async fn delete_passkey(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    s.require_recent_passkey()?;
    let mut tx = state.db.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM passkeys WHERE user_id = ?")
        .bind(&s.user.id)
        .fetch_one(&mut *tx)
        .await?;
    if count <= 1 {
        return Err(ApiError::bad_request("last_passkey", "You need at least one passkey. Add another first."));
    }
    let n = sqlx::query("DELETE FROM passkeys WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(&s.user.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("No such passkey."));
    }
    tx.commit().await?;
    Ok(ok())
}

// ---------------------------------------------------------------------------
// Sessions

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SessionInfo {
    pub id: String,
    pub device: String,
    #[ts(type = "number")]
    pub last_seen_at: i64,
    pub current: bool,
}

pub async fn list_sessions(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<Vec<SessionInfo>>> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id, device, last_seen_at FROM sessions WHERE user_id = ? AND expires_at > ? ORDER BY last_seen_at DESC",
    )
    .bind(&s.user.id)
    .bind(now_ms())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, device, last_seen_at)| SessionInfo { current: id == s.id, id, device, last_seen_at })
            .collect(),
    ))
}

pub async fn revoke_session(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    sqlx::query("DELETE FROM sessions WHERE id = ? AND user_id = ? AND id != ?")
        .bind(id)
        .bind(&s.user.id)
        .bind(&s.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

pub async fn revoke_other_sessions(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<Ok_>> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND id != ?")
        .bind(&s.user.id)
        .bind(&s.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

// ---------------------------------------------------------------------------
// Recovery codes

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoveryEvent {
    #[ts(type = "number")]
    pub at: i64,
    pub message: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoveryStatus {
    #[ts(type = "number")]
    pub left: i64,
    pub events: Vec<RecoveryEvent>,
}

pub async fn recovery_status(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<RecoveryStatus>> {
    let left = recovery::remaining(&state.db, &s.user.id).await?;
    let events: Vec<(i64, String)> =
        sqlx::query_as("SELECT at, message FROM recovery_events WHERE user_id = ? ORDER BY at DESC LIMIT 20")
            .bind(&s.user.id)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(RecoveryStatus {
        left,
        events: events.into_iter().map(|(at, message)| RecoveryEvent { at, message }).collect(),
    }))
}

pub async fn regenerate_codes(
    State(state): State<AppState>,
    Authed(s): Authed,
) -> ApiResult<Json<super::handlers::RecoveryCodes>> {
    s.require_recent_passkey()?;
    let mut tx = state.db.begin().await?;
    let codes = recovery::regenerate(&mut tx, &s.user.id).await?;
    tx.commit().await?;
    Ok(Json(super::handlers::RecoveryCodes { codes }))
}
