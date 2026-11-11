//! Admin endpoints: users, signup and invites, catalog providers, the review
//! queue, shared catalog edits, the audit log, and stats.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppState;
use crate::audit;
use crate::auth::handlers::{Ok_, issue_reset_link, ok};
use crate::auth::session::{Admin, Authed};
use crate::catalog::similar_titles;
use crate::error::{ApiError, ApiResult};
use crate::library::{self, NewEdition, Person, WorkRow};
use crate::providers::{self, ProviderConfig, ProviderName};
use crate::settings::{self, InviteWho, SignupMode};
use crate::util::{DAY_MS, new_id, now_ms, random_token, sha256};

fn actor(a: &Admin) -> Option<(&str, &str)> {
    Some((a.0.user.id.as_str(), a.0.user.username.as_str()))
}

// ---------------------------------------------------------------------------
// Users

#[derive(Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub suspended: bool,
    #[ts(type = "number")]
    pub books: i64,
    #[ts(type = "number")]
    pub created_at: i64,
}

pub async fn users(State(state): State<AppState>, _a: Admin) -> ApiResult<Json<Vec<UserRow>>> {
    let rows = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name, u.is_admin, u.suspended,
                (SELECT COUNT(*) FROM books b WHERE b.user_id = u.id AND b.deleted = 0) AS books, u.created_at
         FROM users u ORDER BY u.created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

async fn active_admins(conn: &mut sqlx::SqliteConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND suspended = 0")
        .fetch_one(conn)
        .await
}

fn last_admin() -> ApiError {
    ApiError::forbidden("That's the last admin. Make someone else an admin first.")
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct UserUpdate {
    pub is_admin: Option<bool>,
    pub suspended: Option<bool>,
}

pub async fn update_user(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    Json(req): Json<UserUpdate>,
) -> ApiResult<Json<Ok_>> {
    // Granting admin hands over everyone's account recovery.
    if req.is_admin == Some(true) {
        a.0.require_recent_confirmation()?;
    }
    let mut tx = state.db.begin().await?;
    let (username, is_admin, suspended): (String, bool, bool) =
        sqlx::query_as("SELECT username, is_admin, suspended FROM users WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("No such user."))?;
    let losing_admin =
        is_admin && !suspended && (req.is_admin == Some(false) || req.suspended == Some(true));
    if losing_admin && active_admins(&mut tx).await? <= 1 {
        return Err(last_admin());
    }
    if let Some(v) = req.is_admin
        && v != is_admin
    {
        sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?")
            .bind(v)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        let what = if v {
            "made {u} an admin"
        } else {
            "removed admin from {u}"
        };
        audit::record(&mut *tx, actor(&a), what.replace("{u}", &username)).await?;
    }
    if let Some(v) = req.suspended
        && v != suspended
    {
        sqlx::query("UPDATE users SET suspended = ? WHERE id = ?")
            .bind(v)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if v {
            sqlx::query("DELETE FROM sessions WHERE user_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
        audit::record(
            &mut *tx,
            actor(&a),
            format!("{} {username}", if v { "suspended" } else { "unsuspended" }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(ok())
}

pub async fn delete_user(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    if id == a.0.user.id {
        return Err(ApiError::bad_request(
            "self",
            "Delete your own account from Settings → Account.",
        ));
    }
    a.0.require_recent_confirmation()?;
    let mut tx = state.db.begin().await?;
    let (username, is_admin): (String, bool) =
        sqlx::query_as("SELECT username, is_admin FROM users WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("No such user."))?;
    if is_admin && active_admins(&mut tx).await? <= 1 {
        return Err(last_admin());
    }
    library::delete_user_data(&mut tx, &id).await?;
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut *tx, actor(&a), format!("deleted {username}")).await?;
    tx.commit().await?;
    Ok(ok())
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Link {
    pub url: String,
}

/// A one-time link for a locked-out user to set a new password (24 hours).
/// Using it also turns their two-factor off.
pub async fn reset_link(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
) -> ApiResult<Json<Link>> {
    // The link takes over the account, so a stolen admin session isn't enough.
    a.0.require_recent_confirmation()?;
    let mut tx = state.db.begin().await?;
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("No such user."))?;
    let token = issue_reset_link(&mut tx, &id, Some((&a.0.user.id, &a.0.user.username))).await?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("issued a password reset link for {username}"),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Link {
        url: format!("{}/reset/{token}", state.config.public_url),
    }))
}

// ---------------------------------------------------------------------------
// Signup and invites

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SignupSettings {
    pub mode: SignupMode,
    pub invite_who: InviteWho,
    #[ts(type = "number")]
    pub invite_limit: i64,
}

pub async fn get_signup(
    State(state): State<AppState>,
    _a: Admin,
) -> ApiResult<Json<SignupSettings>> {
    Ok(Json(SignupSettings {
        mode: settings::signup_mode(&state.db).await?,
        invite_who: settings::invite_who(&state.db).await?,
        invite_limit: settings::invite_limit(&state.db).await?,
    }))
}

pub async fn put_signup(
    State(state): State<AppState>,
    a: Admin,
    Json(req): Json<SignupSettings>,
) -> ApiResult<Json<SignupSettings>> {
    if !(1..=100).contains(&req.invite_limit) {
        return Err(ApiError::bad_request(
            "limit",
            "The invite limit must be between 1 and 100.",
        ));
    }
    let before = settings::signup_mode(&state.db).await?;
    settings::set(&state.db, "signup_mode", &req.mode).await?;
    settings::set(&state.db, "invite_who", &req.invite_who).await?;
    settings::set(&state.db, "invite_limit", &req.invite_limit).await?;
    if before != req.mode {
        let name = match req.mode {
            SignupMode::Open => "Open",
            SignupMode::Invite => "Invite only",
            SignupMode::Closed => "Closed",
        };
        audit::record(&state.db, actor(&a), format!("changed signup to {name}")).await?;
    }
    Ok(Json(req))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct InviteRow {
    pub id: String,
    pub created_by: Option<String>,
    #[ts(type = "number")]
    pub expires_at: i64,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Invites {
    /// Whether the signed-in user may create invites now.
    pub can_create: bool,
    /// Pending invites: all of them for admins, your own otherwise.
    pub pending: Vec<InviteRow>,
}

const INVITE_TTL_MS: i64 = 7 * DAY_MS;

async fn can_invite(state: &AppState, user: &crate::auth::User) -> sqlx::Result<bool> {
    if user.is_admin {
        return Ok(true);
    }
    if settings::invite_who(&state.db).await? != InviteWho::All {
        return Ok(false);
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invites WHERE created_by = ? AND used_at IS NULL AND revoked = 0 AND expires_at > ?",
    )
    .bind(&user.id)
    .bind(now_ms())
    .fetch_one(&state.db)
    .await?;
    Ok(pending < settings::invite_limit(&state.db).await?)
}

pub async fn list_invites(
    State(state): State<AppState>,
    Authed(s): Authed,
) -> ApiResult<Json<Invites>> {
    let rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT i.id, u.username, i.expires_at FROM invites i LEFT JOIN users u ON u.id = i.created_by
         WHERE i.used_at IS NULL AND i.revoked = 0 AND i.expires_at > ? AND (? OR i.created_by = ?)
         ORDER BY i.created_at DESC",
    )
    .bind(now_ms())
    .bind(s.user.is_admin)
    .bind(&s.user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Invites {
        can_create: can_invite(&state, &s.user).await?,
        pending: rows
            .into_iter()
            .map(|(id, created_by, expires_at)| InviteRow {
                id,
                created_by,
                expires_at,
            })
            .collect(),
    }))
}

pub async fn create_invite(
    State(state): State<AppState>,
    Authed(s): Authed,
) -> ApiResult<Json<Link>> {
    if !can_invite(&state, &s.user).await? {
        return Err(ApiError::forbidden(
            "You can't create more invites right now.",
        ));
    }
    if settings::signup_mode(&state.db).await? == SignupMode::Closed {
        return Err(ApiError::forbidden(
            "Signup is closed, so invites wouldn't work.",
        ));
    }
    let token = random_token();
    let now = now_ms();
    sqlx::query("INSERT INTO invites (id, token_hash, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?)")
        .bind(new_id())
        .bind(sha256(&token))
        .bind(&s.user.id)
        .bind(now)
        .bind(now + INVITE_TTL_MS)
        .execute(&state.db)
        .await?;
    Ok(Json(Link {
        url: format!("{}/invite/{token}", state.config.public_url),
    }))
}

pub async fn revoke_invite(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    sqlx::query("UPDATE invites SET revoked = 1 WHERE id = ? AND (? OR created_by = ?)")
        .bind(id)
        .bind(s.user.is_admin)
        .bind(&s.user.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

// ---------------------------------------------------------------------------
// Catalog providers

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ProviderInfo {
    pub name: ProviderName,
    pub label: String,
    pub enabled: bool,
    pub needs_key: bool,
    pub has_key: bool,
}

pub async fn get_providers(
    State(state): State<AppState>,
    _a: Admin,
) -> ApiResult<Json<Vec<ProviderInfo>>> {
    let list = providers::configured(&state.db).await?;
    Ok(Json(
        list.into_iter()
            .map(|p| ProviderInfo {
                label: p.name.label().into(),
                needs_key: p.name.needs_key(),
                has_key: p.key.as_deref().is_some_and(|k| !k.is_empty()),
                name: p.name,
                enabled: p.enabled,
            })
            .collect(),
    ))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ProviderUpdate {
    pub name: ProviderName,
    pub enabled: bool,
    /// `None` keeps the stored key; `""` removes it.
    pub key: Option<String>,
}

pub async fn put_providers(
    State(state): State<AppState>,
    a: Admin,
    Json(req): Json<Vec<ProviderUpdate>>,
) -> ApiResult<Json<Vec<ProviderInfo>>> {
    let current = providers::configured(&state.db).await?;
    let mut next: Vec<ProviderConfig> = Vec::new();
    for u in &req {
        if next.iter().any(|p| p.name == u.name) {
            continue;
        }
        let old_key = current
            .iter()
            .find(|p| p.name == u.name)
            .and_then(|p| p.key.clone());
        let key = match &u.key {
            None => old_key,
            Some(k) if k.trim().is_empty() => None,
            Some(k) => Some(k.trim().to_string()),
        };
        next.push(ProviderConfig {
            name: u.name,
            enabled: u.enabled,
            key,
        });
    }
    // Keep any provider the request left out, at the end, as it was.
    for p in current {
        if !next.iter().any(|n| n.name == p.name) {
            next.push(p);
        }
    }
    settings::set(&state.db, "providers", &next).await?;
    let order: Vec<&str> = next
        .iter()
        .filter(|p| p.enabled)
        .map(|p| p.name.label())
        .collect();
    audit::record(
        &state.db,
        actor(&a),
        format!(
            "set catalog providers to: {}",
            if order.is_empty() {
                "none".into()
            } else {
                order.join(", ")
            }
        ),
    )
    .await?;
    get_providers(State(state), a).await
}

// ---------------------------------------------------------------------------
// Review queue

#[derive(Serialize, TS)]
#[ts(export)]
pub struct QueueItem {
    pub work: WorkRow,
    pub owner: String,
    /// Shared works that might be the same book.
    pub similar: Vec<SimilarRef>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SimilarRef {
    pub id: String,
    pub title: String,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
}

pub async fn queue(State(state): State<AppState>, _a: Admin) -> ApiResult<Json<Vec<QueueItem>>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT w.id, w.owner_id, u.username FROM works w JOIN users u ON u.id = w.owner_id
         WHERE w.suggested = 1 AND w.owner_id IS NOT NULL ORDER BY w.created_at",
    )
    .fetch_all(&state.db)
    .await?;
    let shared: Vec<(String, String, Option<i64>)> =
        sqlx::query_as("SELECT id, title, first_year FROM works WHERE owner_id IS NULL")
            .fetch_all(&state.db)
            .await?;
    let mut conn = state.db.acquire().await?;
    let mut out = Vec::new();
    for (id, owner_id, owner) in rows {
        if let Some(work) = library::load_work(&mut conn, &id, Some(&owner_id)).await? {
            let similar = shared
                .iter()
                .filter(|(_, t, _)| similar_titles(&work.title, t))
                .take(3)
                .map(|(id, title, first_year)| SimilarRef {
                    id: id.clone(),
                    title: title.clone(),
                    first_year: *first_year,
                })
                .collect();
            out.push(QueueItem {
                work,
                owner,
                similar,
            });
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct WorkEdit {
    pub title: String,
    pub people: Vec<Person>,
    #[ts(type = "number | null")]
    pub first_year: Option<i64>,
}

async fn apply_work_edit(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    e: &WorkEdit,
    seq: i64,
) -> ApiResult<()> {
    let title = e.title.trim();
    if title.is_empty() || title.chars().count() > 300 {
        return Err(ApiError::bad_request(
            "title",
            "Enter a title of up to 300 characters.",
        ));
    }
    let people = library::clean_people(&e.people)?;
    if !people.iter().any(|p| p.role == "author") {
        return Err(ApiError::bad_request(
            "people",
            "At least one author is required.",
        ));
    }
    sqlx::query("UPDATE works SET title = ?, first_year = ?, seq = ? WHERE id = ?")
        .bind(title)
        .bind(e.first_year)
        .bind(seq)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    library::insert_people(conn, id, &people).await?;
    Ok(())
}

/// Makes a suggested manual entry part of the shared catalog, optionally edited first.
pub async fn promote(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    Json(edit): Json<Option<WorkEdit>>,
) -> ApiResult<Json<Ok_>> {
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let title: String = sqlx::query_scalar(
        "SELECT title FROM works WHERE id = ? AND owner_id IS NOT NULL AND suggested = 1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("That suggestion is gone."))?;
    if let Some(e) = &edit {
        apply_work_edit(&mut tx, &id, e, seq).await?;
    }
    sqlx::query("UPDATE works SET owner_id = NULL, suggested = 0, seq = ? WHERE id = ?")
        .bind(seq)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE editions SET owner_id = NULL, seq = ? WHERE work_id = ?")
        .bind(seq)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("added \"{title}\" to the shared catalog"),
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct MergeRequest {
    pub into: String,
}

/// Points the owner's book at an existing shared work and drops the duplicate.
pub async fn merge(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    Json(req): Json<MergeRequest>,
) -> ApiResult<Json<Ok_>> {
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let title: String =
        sqlx::query_scalar("SELECT title FROM works WHERE id = ? AND owner_id IS NOT NULL")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("That suggestion is gone."))?;
    let into: String =
        sqlx::query_scalar("SELECT title FROM works WHERE id = ? AND owner_id IS NULL")
            .bind(&req.into)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("No such shared work."))?;
    // Copies keep their edition when the shared work has the same ISBN.
    sqlx::query(
        "UPDATE copies SET edition_id = (SELECT s.id FROM editions s JOIN editions p ON p.isbn13 = s.isbn13
                                          WHERE p.id = copies.edition_id AND s.work_id = ?1 LIMIT 1), seq = ?2
         WHERE edition_id IN (SELECT id FROM editions WHERE work_id = ?3)",
    )
    .bind(&req.into)
    .bind(seq)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE books SET work_id = ?, seq = ? WHERE work_id = ?")
        .bind(&req.into)
        .bind(seq)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM works WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("merged \"{title}\" into \"{into}\""),
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}

pub async fn decline(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
) -> ApiResult<Json<Ok_>> {
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let title: String = sqlx::query_scalar(
        "UPDATE works SET suggested = 0, seq = ? WHERE id = ? AND suggested = 1 RETURNING title",
    )
    .bind(seq)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("That suggestion is gone."))?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("declined \"{title}\" for the shared catalog"),
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}

// ---------------------------------------------------------------------------
// Shared catalog editing

#[derive(Deserialize)]
pub struct CatalogQuery {
    q: String,
}

pub async fn catalog_search(
    State(state): State<AppState>,
    _a: Admin,
    Query(q): Query<CatalogQuery>,
) -> ApiResult<Json<Vec<WorkRow>>> {
    let term = q.q.trim().to_lowercase();
    let isbn = mau_core::isbn::normalize(&term);
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM works WHERE owner_id IS NULL AND (lower(title) LIKE ?1
            OR id IN (SELECT work_id FROM work_people WHERE lower(name) LIKE ?1)
            OR id IN (SELECT work_id FROM editions WHERE isbn13 = ?2))
         ORDER BY title LIMIT 20",
    )
    .bind(format!("%{term}%"))
    .bind(isbn.unwrap_or_default())
    .fetch_all(&state.db)
    .await?;
    let mut conn = state.db.acquire().await?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(w) = library::load_work(&mut conn, &id, None).await? {
            out.push(w);
        }
    }
    Ok(Json(out))
}

pub async fn edit_work(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    Json(e): Json<WorkEdit>,
) -> ApiResult<Json<WorkRow>> {
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let shared: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE id = ? AND owner_id IS NULL)")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
    if !shared {
        return Err(ApiError::not_found("No such shared work."));
    }
    apply_work_edit(&mut tx, &id, &e, seq).await?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("edited \"{}\" in the shared catalog", e.title.trim()),
    )
    .await?;
    let w = library::load_work(&mut tx, &id, None)
        .await?
        .ok_or_else(|| ApiError::internal("work vanished"))?;
    tx.commit().await?;
    Ok(Json(w))
}

pub async fn edit_edition(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    Json(e): Json<NewEdition>,
) -> ApiResult<Json<Ok_>> {
    let isbn = match e.isbn13.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(
            mau_core::isbn::normalize(raw)
                .ok_or_else(|| ApiError::bad_request("isbn", "That ISBN doesn't look right."))?,
        ),
        None => None,
    };
    let binding = e
        .binding
        .as_deref()
        .filter(|b| library::FORMATS.contains(b));
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let n = sqlx::query(
        "UPDATE editions SET publisher = ?, binding = ?, year = ?, isbn13 = ?, pages = ?, seq = ? WHERE id = ? AND owner_id IS NULL",
    )
    .bind(e.publisher.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(binding)
    .bind(e.year)
    .bind(isbn)
    .bind(e.pages.filter(|p| *p > 0))
    .bind(seq)
    .bind(&id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("No such shared edition."));
    }
    sqlx::query("UPDATE works SET seq = ? WHERE id = (SELECT work_id FROM editions WHERE id = ?)")
        .bind(seq)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        actor(&a),
        "edited an edition in the shared catalog",
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}

pub async fn set_work_cover(
    State(state): State<AppState>,
    a: Admin,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<Json<WorkRow>> {
    if body.len() > crate::covers::MAX_BYTES {
        return Err(ApiError::bad_request(
            "too_big",
            "Choose an image under 10 MB.",
        ));
    }
    let hash = crate::covers::store(&state.config.data_dir, body.to_vec()).await?;
    let mut tx = state.db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let title: String = sqlx::query_scalar(
        "UPDATE works SET cover = ?, seq = ? WHERE id = ? AND owner_id IS NULL RETURNING title",
    )
    .bind(&hash)
    .bind(seq)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("No such shared work."))?;
    audit::record(
        &mut *tx,
        actor(&a),
        format!("replaced the cover of \"{title}\""),
    )
    .await?;
    let w = library::load_work(&mut tx, &id, None)
        .await?
        .ok_or_else(|| ApiError::internal("work vanished"))?;
    tx.commit().await?;
    Ok(Json(w))
}

// ---------------------------------------------------------------------------
// Audit log and stats

#[derive(Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct AuditEntry {
    #[ts(type = "number")]
    pub at: i64,
    pub actor_name: String,
    pub action: String,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    before: Option<i64>,
}

pub async fn audit_log(
    State(state): State<AppState>,
    _a: Admin,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    let rows = sqlx::query_as("SELECT at, actor_name, action FROM audit_log WHERE at < ? ORDER BY at DESC, id DESC LIMIT 200")
        .bind(q.before.unwrap_or(i64::MAX))
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Stats {
    #[ts(type = "number")]
    pub users: i64,
    #[ts(type = "number")]
    pub books: i64,
    #[ts(type = "number")]
    pub shared_works: i64,
    #[ts(type = "number")]
    pub private_works: i64,
    #[ts(type = "number")]
    pub cover_bytes: u64,
    #[ts(type = "number")]
    pub database_bytes: u64,
}

pub async fn stats(State(state): State<AppState>, _a: Admin) -> ApiResult<Json<Stats>> {
    let count = |sql: &'static str| {
        let db = state.db.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&db).await }
    };
    let dir = crate::covers::dir(&state.config.data_dir);
    let cover_bytes = tokio::task::spawn_blocking(move || {
        std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(|e| e.ok()?.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    let db_path = state.config.database_path();
    let database_bytes = ["", "-wal"]
        .iter()
        .filter_map(|s| std::fs::metadata(format!("{}{s}", db_path.display())).ok())
        .map(|m| m.len())
        .sum();
    Ok(Json(Stats {
        users: count("SELECT COUNT(*) FROM users").await?,
        books: count("SELECT COUNT(*) FROM books WHERE deleted = 0").await?,
        shared_works: count("SELECT COUNT(*) FROM works WHERE owner_id IS NULL").await?,
        private_works: count("SELECT COUNT(*) FROM works WHERE owner_id IS NOT NULL").await?,
        cover_bytes,
        database_bytes,
    }))
}

#[cfg(test)]
mod tests {
    use crate::app_tests::TestApp;
    use serde_json::json;

    #[tokio::test]
    async fn last_admin_is_protected_and_suspension_signs_out() {
        let app = TestApp::new().await;
        let (admin_id, admin) = app.user("boss", true).await;
        let (user_id, user) = app.user("juno", false).await;
        let (status, _, _) = app
            .request(
                "PATCH",
                &format!("/api/admin/users/{admin_id}"),
                Some(json!({"is_admin": false})),
                Some(&admin),
            )
            .await;
        assert_eq!(status, 403);
        // Regular users can't use admin endpoints.
        let (status, _) = app.get("/api/admin/users", Some(&user)).await;
        assert_eq!(status, 403);
        let (status, _, _) = app
            .request(
                "PATCH",
                &format!("/api/admin/users/{user_id}"),
                Some(json!({"suspended": true})),
                Some(&admin),
            )
            .await;
        assert_eq!(status, 200);
        let (status, _) = app.get("/api/sessions", Some(&user)).await;
        assert_eq!(status, 401);
        let (_, log) = app.get("/api/admin/audit", Some(&admin)).await;
        assert_eq!(log[0]["action"], "suspended juno");
    }

    #[tokio::test]
    async fn invites_follow_the_signup_settings() {
        let app = TestApp::new().await;
        let (_, admin) = app.user("boss", true).await;
        let (_, user) = app.user("juno", false).await;
        let (_, inv) = app.get("/api/invites", Some(&user)).await;
        assert_eq!(inv["can_create"], false);
        let (status, _, _) = app
            .request(
                "PUT",
                "/api/admin/signup",
                Some(json!({"mode": "invite", "invite_who": "all", "invite_limit": 1})),
                Some(&admin),
            )
            .await;
        assert_eq!(status, 200);
        let (status, link) = app.post("/api/invites", json!({}), Some(&user)).await;
        assert_eq!(status, 200);
        assert!(link["url"].as_str().unwrap().contains("/invite/"));
        let (status, _) = app.post("/api/invites", json!({}), Some(&user)).await;
        assert_eq!(status, 403, "limit of 1 pending invite");
    }

    #[tokio::test]
    async fn suggested_entries_can_be_promoted_or_merged() {
        let app = TestApp::new().await;
        let (_, admin) = app.user("boss", true).await;
        let (_, user) = app.user("juno", false).await;
        let h = "1000000000000-0000-aaa";
        for (wid, bid, title) in [
            ("p1", "b1", "Earthsea Revisioned"),
            ("p2", "b2", "A Wizard of Earthsea"),
        ] {
            let (_, res) = app
                .post(
                    "/api/sync",
                    json!({"since": 0, "ops": [
                        {"id": format!("o{wid}"), "hlc": h, "op": {"type": "work.create", "work": {"id": wid, "title": title, "first_year": null, "people": [{"name": "Ursula K. Le Guin", "role": "author"}], "edition": null}}},
                        {"id": format!("x{bid}"), "hlc": h, "op": {"type": "book.create", "id": bid, "work_id": wid, "rank": "V", "added_at": 1}}
                    ]}),
                    Some(&user),
                )
                .await;
            assert_eq!(res["dropped"], json!([]), "{res}");
            let (status, _) = app
                .post(
                    &format!("/api/catalog/works/{wid}/suggest"),
                    json!({}),
                    Some(&user),
                )
                .await;
            assert_eq!(status, 200);
        }
        let (_, q) = app.get("/api/admin/queue", Some(&admin)).await;
        assert_eq!(q.as_array().unwrap().len(), 2);
        // Promote the first (renamed), then merge the second into it.
        let (status, _) = app
            .post("/api/admin/queue/p1/promote", json!({"title": "Earthsea Revisioned", "people": [{"name": "Ursula K. Le Guin", "role": "author"}], "first_year": 1993}), Some(&admin))
            .await;
        assert_eq!(status, 200);
        let (status, _) = app
            .post(
                "/api/admin/queue/p2/merge",
                json!({"into": "p1"}),
                Some(&admin),
            )
            .await;
        assert_eq!(status, 200);
        let (_, res) = app
            .post("/api/sync", json!({"since": 0, "ops": []}), Some(&user))
            .await;
        let books = res["changes"]["books"].as_array().unwrap();
        assert!(books.iter().all(|b| b["work_id"] == "p1"));
        assert_eq!(res["changes"]["works"][0]["private"], false);
        assert_eq!(res["changes"]["works"][0]["first_year"], 1993);
    }
}
