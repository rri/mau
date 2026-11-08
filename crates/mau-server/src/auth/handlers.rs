//! Sign-up, sign-in, recovery, and password confirmation endpoints.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::pending::Pending;
use super::session::{self, Authed};
use super::{User, password, recovery, totp};
use crate::app::AppState;
use crate::audit;
use crate::error::{ApiError, ApiResult};
use crate::settings::{self, SignupMode};
use crate::util::{DAY_MS, device_label, new_id, now_ms, sha256};

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub time_zone: String,
    pub is_admin: bool,
    /// Authenticator-app codes are on.
    pub two_factor: bool,
}

impl From<&User> for Me {
    fn from(u: &User) -> Self {
        Me {
            id: u.id.clone(),
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            bio: u.bio.clone(),
            time_zone: u.time_zone.clone(),
            is_admin: u.is_admin,
            two_factor: u.two_factor(),
        }
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthStatus {
    pub me: Option<Me>,
    pub signup_mode: SignupMode,
    /// No admin exists yet; the setup link from the server log is needed.
    pub setup_required: bool,
}

pub fn is_secure(state: &AppState) -> bool {
    state.config.public_url.starts_with("https://")
}

fn user_agent(headers: &HeaderMap) -> String {
    device_label(
        headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    )
}

fn with_cookie<T: Serialize>(state: &AppState, token: &str, body: T) -> Response {
    (
        AppendHeaders([(
            header::SET_COOKIE,
            session::cookie_header(token, is_secure(state)),
        )]),
        Json(body),
    )
        .into_response()
}

async fn admin_exists(db: &sqlx::SqlitePool) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_admin = 1)")
        .fetch_one(db)
        .await
}

pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<AuthStatus>> {
    let session = match session::token_from_headers(&headers) {
        Some(t) => session::lookup(&state.db, &t).await?,
        None => None,
    };
    Ok(Json(AuthStatus {
        me: session.as_ref().map(|s| Me::from(&s.user)),
        signup_mode: settings::signup_mode(&state.db).await?,
        setup_required: !admin_exists(&state.db).await?,
    }))
}

// ---------------------------------------------------------------------------
// Usernames

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct UsernameCheck {
    pub username: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct UsernameAvailability {
    pub available: bool,
    pub message: String,
}

/// Whether `name` (already canonical) is free for `for_user` (or a new user).
pub async fn username_taken(
    db: impl sqlx::SqliteExecutor<'_>,
    name: &str,
    for_user: Option<&str>,
) -> sqlx::Result<bool> {
    let me = for_user.unwrap_or("");
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE username = ?1 AND id != ?2)
             OR EXISTS(SELECT 1 FROM username_reservations WHERE username = ?1 AND until > ?3 AND user_id != ?2)",
    )
    .bind(name)
    .bind(me)
    .bind(now_ms())
    .fetch_one(db)
    .await
}

pub async fn username_available(
    State(state): State<AppState>,
    Json(req): Json<UsernameCheck>,
) -> ApiResult<Json<UsernameAvailability>> {
    let res = match mau_core::username::validate(&req.username) {
        Err(e) => UsernameAvailability {
            available: false,
            message: e.message().into(),
        },
        Ok(name) if username_taken(&state.db, &name, None).await? => UsernameAvailability {
            available: false,
            message: format!("@{name} is taken."),
        },
        Ok(name) => UsernameAvailability {
            available: true,
            message: format!("@{name} is available."),
        },
    };
    Ok(Json(res))
}

// ---------------------------------------------------------------------------
// Invites and setup

#[derive(Serialize, TS)]
#[ts(export)]
pub struct InviteInfo {
    pub valid: bool,
    pub inviter: Option<String>,
}

/// The invite's ID if the token is usable now.
async fn live_invite(
    db: impl sqlx::SqliteExecutor<'_>,
    token: &str,
) -> sqlx::Result<Option<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT i.id, u.display_name FROM invites i LEFT JOIN users u ON u.id = i.created_by
         WHERE i.token_hash = ? AND i.used_at IS NULL AND i.revoked = 0 AND i.expires_at > ?",
    )
    .bind(sha256(token))
    .bind(now_ms())
    .fetch_optional(db)
    .await
}

pub async fn invite_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<InviteInfo>> {
    let invite = live_invite(&state.db, &token).await?;
    let closed = settings::signup_mode(&state.db).await? == SignupMode::Closed;
    Ok(Json(match invite {
        Some((_, inviter)) if !closed => InviteInfo {
            valid: true,
            inviter,
        },
        _ => InviteInfo {
            valid: false,
            inviter: None,
        },
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct SetupCheck {
    pub token: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Valid {
    pub valid: bool,
}

fn setup_token_matches(state: &AppState, token: &str) -> bool {
    state.setup_token.lock().unwrap().as_deref() == Some(token)
}

pub async fn setup_check(
    State(state): State<AppState>,
    Json(req): Json<SetupCheck>,
) -> ApiResult<Json<Valid>> {
    let valid = setup_token_matches(&state, &req.token) && !admin_exists(&state.db).await?;
    Ok(Json(Valid { valid }))
}

// ---------------------------------------------------------------------------
// Registration

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct Register {
    pub username: String,
    pub display_name: String,
    pub time_zone: String,
    pub password: String,
    pub invite: Option<String>,
    pub setup: Option<String>,
}

pub fn clean_display_name(s: &str) -> ApiResult<String> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > 80 {
        return Err(ApiError::bad_request(
            "display_name",
            "Enter a display name of up to 80 characters.",
        ));
    }
    Ok(s.to_string())
}

pub fn clean_time_zone(s: &str) -> ApiResult<String> {
    let s = s.trim();
    if s.is_empty()
        || s.len() > 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
    {
        return Err(ApiError::bad_request("time_zone", "Unknown time zone."));
    }
    Ok(s.to_string())
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoveryCodes {
    pub codes: Vec<String>,
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<Register>,
) -> ApiResult<Response> {
    let username = mau_core::username::validate(&req.username)
        .map_err(|e| ApiError::bad_request("username", e.message()))?;
    let display_name = clean_display_name(&req.display_name)?;
    let time_zone = clean_time_zone(&req.time_zone)?;
    password::validate(&req.password, &username)?;

    let mode = settings::signup_mode(&state.db).await?;
    let mut first_admin = false;
    let mut invite_id = None;
    if let Some(token) = &req.setup {
        if !setup_token_matches(&state, token) || admin_exists(&state.db).await? {
            return Err(ApiError::forbidden(
                "This setup link has already been used.",
            ));
        }
        first_admin = true;
    } else if !admin_exists(&state.db).await? {
        return Err(ApiError::forbidden(
            "This instance isn't set up yet. Use the setup link printed in the server log.",
        ));
    } else if let Some(token) = &req.invite {
        if mode == SignupMode::Closed {
            return Err(ApiError::forbidden("Signup is closed on this instance."));
        }
        let (id, _) = live_invite(&state.db, token).await?.ok_or_else(|| {
            ApiError::forbidden("This invite link has expired or has already been used.")
        })?;
        invite_id = Some(id);
    } else if mode != SignupMode::Open {
        return Err(ApiError::forbidden(
            "You need an invite link to create an account.",
        ));
    }

    let hash = password::hash(&req.password).await;
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    if username_taken(&mut *tx, &username, None).await? {
        return Err(ApiError::conflict(
            "username_taken",
            format!("@{username} is taken."),
        ));
    }
    if first_admin {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_admin = 1)")
                .fetch_one(&mut *tx)
                .await?;
        if exists {
            return Err(ApiError::forbidden(
                "This setup link has already been used.",
            ));
        }
    }
    if let Some(invite_id) = &invite_id {
        let claimed = sqlx::query(
            "UPDATE invites SET used_at = ? WHERE id = ? AND used_at IS NULL AND revoked = 0 AND expires_at > ?",
        )
        .bind(now)
        .bind(invite_id)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if claimed == 0 {
            return Err(ApiError::forbidden(
                "This invite link has expired or has already been used.",
            ));
        }
    }

    let user_id = new_id();
    sqlx::query(
        "INSERT INTO users (id, username, display_name, time_zone, is_admin, password_hash, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&display_name)
    .bind(&time_zone)
    .bind(first_admin)
    .bind(&hash)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if let Some(invite_id) = &invite_id {
        sqlx::query("UPDATE invites SET used_by = ? WHERE id = ?")
            .bind(&user_id)
            .bind(invite_id)
            .execute(&mut *tx)
            .await?;
    }
    let codes = recovery::regenerate(&mut tx, &user_id).await?;
    let action = if first_admin {
        format!("{username} created the instance's first (admin) account")
    } else {
        format!("{username} signed up")
    };
    audit::record(&mut *tx, Some((&user_id, &username)), action).await?;
    tx.commit().await?;

    if first_admin {
        state.setup_token.lock().unwrap().take();
        tracing::info!("admin account created; the setup link no longer works");
    }
    let token = session::create(&state.db, &user_id, &user_agent(&headers), true).await?;
    Ok(with_cookie(&state, &token, RecoveryCodes { codes }))
}

// ---------------------------------------------------------------------------
// Sign-in

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Ok_ {
    pub ok: bool,
}

pub fn ok() -> Json<Ok_> {
    Json(Ok_ { ok: true })
}

async fn user_by_name(db: &sqlx::SqlitePool, name: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE username = ?"
    ))
    .bind(name.trim().to_ascii_lowercase())
    .fetch_optional(db)
    .await
}

async fn user_by_id(db: &sqlx::SqlitePool, id: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

fn suspended() -> ApiError {
    ApiError::forbidden("This account is suspended. Contact an admin.")
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct Login {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct LoginResult {
    /// The password was right and an authenticator code is needed next;
    /// send it with this ticket to `/auth/login/code`.
    pub code_ticket: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<Login>,
) -> ApiResult<Response> {
    let wrong = || {
        ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "bad_login",
            "That username and password don't match.",
        )
    };
    let name = req.username.trim().to_ascii_lowercase();
    // Besides the per-IP limit, slow down guessing against one account.
    state.limits.check("login-user", &name, 10)?;
    let user = user_by_name(&state.db, &name).await?;
    let hash = user.as_ref().and_then(|u| u.password_hash.as_deref());
    if !password::verify(&req.password, hash).await {
        return Err(wrong());
    }
    let user = user.ok_or_else(wrong)?;
    if user.suspended {
        return Err(suspended());
    }
    if user.two_factor() {
        let ticket = state.pending.insert(Pending::SignIn {
            user_id: user.id,
            attempts: 0,
        });
        return Ok(Json(LoginResult {
            code_ticket: Some(ticket),
        })
        .into_response());
    }
    let token = session::create(&state.db, &user.id, &user_agent(&headers), true).await?;
    Ok(with_cookie(
        &state,
        &token,
        LoginResult { code_ticket: None },
    ))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct LoginCode {
    pub ticket: String,
    /// An authenticator code, or one of the recovery codes.
    pub code: String,
}

/// Checks an authenticator code, or else a recovery code, for `user`.
/// Accepted authenticator codes can't be used again.
async fn check_second_factor(
    state: &AppState,
    user: &User,
    code: &str,
    device: &str,
) -> ApiResult<bool> {
    let Some(secret) = &user.totp_secret else {
        return Ok(true);
    };
    if totp::looks_like_code(code) {
        let Some(step) = totp::verify(secret, code, now_ms(), user.totp_last_step) else {
            return Ok(false);
        };
        // Only one request can claim a given step.
        let claimed = sqlx::query(
            "UPDATE users SET totp_last_step = ?1 WHERE id = ?2 AND (totp_last_step IS NULL OR totp_last_step < ?1)",
        )
        .bind(step)
        .bind(&user.id)
        .execute(&state.db)
        .await?
        .rows_affected();
        return Ok(claimed == 1);
    }
    let mut tx = state.db.begin().await?;
    if !recovery::consume(&mut tx, &user.id, code).await? {
        return Ok(false);
    }
    record_recovery_event(
        &mut *tx,
        &user.id,
        format!("A recovery code was used instead of an authenticator code on {device}."),
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn bad_code() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNAUTHORIZED,
        "bad_code",
        "That code didn't work. Codes change every 30 seconds; try the current one.",
    )
}

pub async fn login_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginCode>,
) -> ApiResult<Response> {
    let start_over = || {
        ApiError::bad_request(
            "ticket_expired",
            "Please sign in again: that took too long, or there were too many wrong codes.",
        )
    };
    let Some(Pending::SignIn { user_id, .. }) = state.pending.get(&req.ticket) else {
        return Err(start_over());
    };
    let user = user_by_id(&state.db, &user_id)
        .await?
        .ok_or_else(start_over)?;
    let device = user_agent(&headers);
    if !check_second_factor(&state, &user, &req.code, &device).await? {
        if !state.pending.count_attempt(&req.ticket) {
            return Err(start_over());
        }
        return Err(bad_code());
    }
    state.pending.remove(&req.ticket);
    if user.suspended {
        return Err(suspended());
    }
    let token = session::create(&state.db, &user.id, &device, true).await?;
    Ok(with_cookie(&state, &token, ok().0))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = session::token_from_headers(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(sha256(&token))
            .execute(&state.db)
            .await?;
    }
    Ok((
        AppendHeaders([(
            header::SET_COOKIE,
            session::clear_cookie_header(is_secure(&state)),
        )]),
        Json(Ok_ { ok: true }),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Forgotten passwords: recovery codes and reset links

pub async fn record_recovery_event(
    db: impl sqlx::SqliteExecutor<'_>,
    user_id: &str,
    message: String,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO recovery_events (user_id, at, message) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(now_ms())
        .bind(message)
        .execute(db)
        .await?;
    Ok(())
}

/// Sets a new password and signs out every session.
async fn reset_password(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    hash: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RecoveryReset {
    pub username: String,
    pub code: String,
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoveryDone {
    #[ts(type = "number")]
    pub codes_left: i64,
}

/// A recovery code sets a new password and signs in. Two-factor stays as it
/// was; the code stands in for it this time.
pub async fn recovery_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RecoveryReset>,
) -> ApiResult<Response> {
    let wrong = || {
        ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "bad_recovery_code",
            "That code and username don't match an account. Check for typos; each code works only once.",
        )
    };
    let name = req.username.trim().to_ascii_lowercase();
    state.limits.check("recovery-user", &name, 5)?;
    let user = user_by_name(&state.db, &name).await?.ok_or_else(wrong)?;
    password::validate(&req.password, &user.username)?;
    let hash = password::hash(&req.password).await;
    let device = user_agent(&headers);
    let mut tx = state.db.begin().await?;
    if !recovery::consume(&mut tx, &user.id, &req.code).await? {
        return Err(wrong());
    }
    if user.suspended {
        return Err(suspended());
    }
    reset_password(&mut tx, &user.id, &hash).await?;
    record_recovery_event(
        &mut *tx,
        &user.id,
        format!("A recovery code was used to set a new password on {device}."),
    )
    .await?;
    let left = recovery::remaining(&mut *tx, &user.id).await?;
    tx.commit().await?;
    let token = session::create(&state.db, &user.id, &device, true).await?;
    Ok(with_cookie(
        &state,
        &token,
        RecoveryDone { codes_left: left },
    ))
}

/// How long reset links work.
pub const RESET_LINK_TTL_MS: i64 = DAY_MS;

/// Makes a one-time reset link for `user_id` and returns its token.
/// `by` is the admin who asked for it; `None` means the `mau reset-password`
/// command.
pub async fn issue_reset_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    by: Option<(&str, &str)>,
) -> sqlx::Result<String> {
    let token = crate::util::random_token();
    let now = now_ms();
    // Only the newest link works.
    sqlx::query("UPDATE reset_links SET used_at = ? WHERE user_id = ? AND used_at IS NULL")
        .bind(now)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO reset_links (token_hash, user_id, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?)")
        .bind(sha256(&token))
        .bind(user_id)
        .bind(by.map(|(id, _)| id))
        .bind(now)
        .bind(now + RESET_LINK_TTL_MS)
        .execute(&mut **tx)
        .await?;
    let who = match by {
        Some((_, name)) => format!("An admin ({name})"),
        None => "The server administrator".to_string(),
    };
    record_recovery_event(
        &mut **tx,
        user_id,
        format!("{who} issued a one-time link for setting a new password."),
    )
    .await?;
    Ok(token)
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ResetInfo {
    pub valid: bool,
    pub username: Option<String>,
    /// Using the link turns two-factor off; the page says so.
    pub two_factor: bool,
}

async fn live_reset_link(
    db: impl sqlx::SqliteExecutor<'_>,
    token: &str,
) -> sqlx::Result<Option<(String, String, bool)>> {
    sqlx::query_as(
        "SELECT r.user_id, u.username, u.totp_secret IS NOT NULL FROM reset_links r JOIN users u ON u.id = r.user_id
         WHERE r.token_hash = ? AND r.used_at IS NULL AND r.expires_at > ?",
    )
    .bind(sha256(token))
    .bind(now_ms())
    .fetch_optional(db)
    .await
}

pub async fn reset_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<ResetInfo>> {
    let link = live_reset_link(&state.db, &token).await?;
    Ok(Json(match link {
        Some((_, username, two_factor)) => ResetInfo {
            valid: true,
            username: Some(username),
            two_factor,
        },
        None => ResetInfo {
            valid: false,
            username: None,
            two_factor: false,
        },
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ResetUse {
    pub password: String,
}

/// Uses a reset link: sets the new password, turns two-factor off (the
/// person may have lost their authenticator too), and signs in.
pub async fn reset_use(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Json(req): Json<ResetUse>,
) -> ApiResult<Response> {
    let gone = || ApiError::forbidden("This link has expired or has already been used.");
    let (user_id, username, _) = live_reset_link(&state.db, &token).await?.ok_or_else(gone)?;
    password::validate(&req.password, &username)?;
    let hash = password::hash(&req.password).await;
    let device = user_agent(&headers);
    let mut tx = state.db.begin().await?;
    let claimed = sqlx::query(
        "UPDATE reset_links SET used_at = ? WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(now_ms())
    .bind(sha256(&token))
    .bind(now_ms())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if claimed == 0 {
        return Err(gone());
    }
    reset_password(&mut tx, &user_id, &hash).await?;
    sqlx::query("UPDATE users SET totp_secret = NULL, totp_last_step = NULL WHERE id = ?")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    record_recovery_event(
        &mut *tx,
        &user_id,
        format!("A one-time link was used to set a new password on {device}. Two-factor is off."),
    )
    .await?;
    tx.commit().await?;
    let token = session::create(&state.db, &user_id, &device, true).await?;
    Ok(with_cookie(&state, &token, ok().0))
}

// ---------------------------------------------------------------------------
// Confirmation for sensitive actions

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct Confirm {
    pub password: String,
    /// Needed when two-factor is on: an authenticator or recovery code.
    pub code: Option<String>,
}

pub async fn confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Authed(s): Authed,
    Json(req): Json<Confirm>,
) -> ApiResult<Json<Ok_>> {
    state.limits.check("confirm-user", &s.user.id, 5)?;
    if !password::verify(&req.password, s.user.password_hash.as_deref()).await {
        return Err(ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "bad_password",
            "That password isn't right.",
        ));
    }
    if s.user.two_factor() {
        let code = req.code.as_deref().unwrap_or("").trim();
        if code.is_empty() {
            return Err(ApiError::bad_request(
                "code_required",
                "Enter the code from your authenticator app.",
            ));
        }
        if !check_second_factor(&state, &s.user, code, &user_agent(&headers)).await? {
            return Err(bad_code());
        }
    }
    sqlx::query("UPDATE sessions SET reauth_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(&s.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}
