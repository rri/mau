//! Publishing parts of a library: each share is public (anyone with the link,
//! no sign-in) and listed on the owner's profile. See `public.rs` for pages.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppState;
use crate::auth::handlers::{Ok_, ok};
use crate::auth::session::Authed;
use crate::error::{ApiError, ApiResult};
use crate::util::{new_id, now_ms, random_token};

#[derive(Debug, Clone, Serialize, Deserialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Share {
    pub id: String,
    /// finished, active, tag, or catalog
    pub kind: String,
    pub tag: Option<String>,
    pub include_ratings: bool,
    /// The share's own link is /s/<token>.
    pub token: String,
}

pub async fn list(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<Vec<Share>>> {
    let rows = sqlx::query_as("SELECT id, kind, tag, include_ratings, token FROM shares WHERE user_id = ? ORDER BY created_at")
        .bind(&s.user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct NewShare {
    pub kind: String,
    pub tag: Option<String>,
    pub include_ratings: bool,
}

/// Link tokens are shorter than session tokens but still unguessable (128 bits).
fn link_token() -> String {
    random_token()[..22].to_string()
}

pub async fn create(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<NewShare>,
) -> ApiResult<Json<Share>> {
    if !["finished", "active", "tag", "catalog"].contains(&req.kind.as_str()) {
        return Err(ApiError::bad_request("kind", "Unknown kind of share."));
    }
    let tag = match req.kind.as_str() {
        "tag" => Some(
            req.tag
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| ApiError::bad_request("tag", "Choose a tag to share."))?
                .to_string(),
        ),
        _ => None,
    };
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM shares WHERE user_id = ? AND kind = ? AND tag IS ?)",
    )
    .bind(&s.user.id)
    .bind(&req.kind)
    .bind(&tag)
    .fetch_one(&state.db)
    .await?;
    if exists {
        return Err(ApiError::conflict("exists", "That's already shared."));
    }
    let share = Share {
        id: new_id(),
        kind: req.kind,
        tag,
        include_ratings: req.include_ratings,
        token: link_token(),
    };
    sqlx::query("INSERT INTO shares (id, user_id, kind, tag, include_ratings, token, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&share.id)
        .bind(&s.user.id)
        .bind(&share.kind)
        .bind(&share.tag)
        .bind(share.include_ratings)
        .bind(&share.token)
        .bind(now_ms())
        .execute(&state.db)
        .await?;
    Ok(Json(share))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ShareUpdate {
    pub include_ratings: Option<bool>,
    /// Replace the link; the old one stops working.
    pub new_link: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    Json(req): Json<ShareUpdate>,
) -> ApiResult<Json<Share>> {
    let mut share: Share = sqlx::query_as(
        "SELECT id, kind, tag, include_ratings, token FROM shares WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&s.user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("No such share."))?;
    if let Some(r) = req.include_ratings {
        share.include_ratings = r;
    }
    if req.new_link == Some(true) {
        share.token = link_token();
    }
    sqlx::query("UPDATE shares SET include_ratings = ?, token = ? WHERE id = ?")
        .bind(share.include_ratings)
        .bind(&share.token)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(share))
}

pub async fn delete(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    sqlx::query("DELETE FROM shares WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(&s.user.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}
