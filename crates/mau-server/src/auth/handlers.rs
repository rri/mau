//! Sign-up, sign-in, recovery, and passkey confirmation endpoints.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential, Uuid, WebauthnError};
use webauthn_rs_proto::ResidentKeyRequirement;

use super::ceremony::{Ceremony, NewAccount};
use super::session::{self, AnySession, Authed};
use super::{User, recovery};
use crate::app::AppState;
use crate::audit;
use crate::error::{ApiError, ApiResult};
use crate::settings::{self, SignupMode};
use crate::util::{DAY_MS, b64url, device_label, new_id, now_ms, sha256};

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub time_zone: String,
    pub is_admin: bool,
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
        }
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthStatus {
    pub me: Option<Me>,
    /// Signed in with a recovery code or enrollment link; must add a passkey.
    pub restricted: bool,
    pub signup_mode: SignupMode,
    /// No admin exists yet; the setup link from the server log is needed.
    pub setup_required: bool,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CeremonyStart {
    pub challenge: String,
    /// WebAuthn options (`{ publicKey: ... }`) with binary fields as base64url.
    #[ts(type = "any")]
    pub options: serde_json::Value,
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
        restricted: session.as_ref().is_some_and(|s| s.restricted),
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
pub struct RegisterStart {
    pub username: String,
    pub display_name: String,
    pub time_zone: String,
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

/// webauthn-rs asks for non-discoverable credentials; mau's username-less
/// sign-in needs discoverable ones. See docs/DECISIONS.md.
fn require_resident_key(ccr: &mut webauthn_rs::prelude::CreationChallengeResponse) {
    if let Some(sel) = ccr.public_key.authenticator_selection.as_mut() {
        sel.resident_key = Some(ResidentKeyRequirement::Required);
        sel.require_resident_key = true;
    }
}

pub async fn register_start(
    State(state): State<AppState>,
    Json(req): Json<RegisterStart>,
) -> ApiResult<Json<CeremonyStart>> {
    let username = mau_core::username::validate(&req.username)
        .map_err(|e| ApiError::bad_request("username", e.message()))?;
    let display_name = clean_display_name(&req.display_name)?;
    let time_zone = clean_time_zone(&req.time_zone)?;

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

    if username_taken(&state.db, &username, None).await? {
        return Err(ApiError::conflict(
            "username_taken",
            format!("@{username} is taken."),
        ));
    }

    let webauthn_id = Uuid::new_v4();
    let (mut ccr, reg) = state
        .webauthn
        .start_passkey_registration(webauthn_id, &username, &display_name, None)
        .map_err(ApiError::internal)?;
    require_resident_key(&mut ccr);
    let account = NewAccount {
        username,
        display_name,
        time_zone,
        webauthn_id: webauthn_id.to_string(),
        invite_id,
        first_admin,
    };
    let challenge = state.ceremonies.insert(Ceremony::Register {
        account,
        state: reg,
    });
    Ok(Json(CeremonyStart {
        challenge,
        options: serde_json::to_value(&ccr)?,
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RegisterFinish {
    pub challenge: String,
    #[ts(type = "any")]
    pub credential: RegisterPublicKeyCredential,
    pub passkey_name: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoveryCodes {
    pub codes: Vec<String>,
}

pub fn clean_passkey_name(name: &str, fallback: &str) -> String {
    let n: String = name.trim().chars().take(60).collect();
    if n.is_empty() {
        fallback.to_string()
    } else {
        n
    }
}

fn registration_failed(e: WebauthnError) -> ApiError {
    match e {
        // Security keys without a PIN can't verify the user, and some
        // browsers don't offer to set one.
        WebauthnError::UserNotVerified => ApiError::bad_request(
            "passkey_no_pin",
            "This security key didn't ask for a PIN, and mau requires one. Set a PIN on the key, then try again.",
        ),
        e => ApiError::bad_request(
            "passkey",
            format!("The passkey couldn't be registered ({e})."),
        ),
    }
}

fn expired() -> ApiError {
    ApiError::bad_request("challenge_expired", "That took too long. Please try again.")
}

pub async fn register_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterFinish>,
) -> ApiResult<Response> {
    let Some(Ceremony::Register {
        account,
        state: reg,
    }) = state.ceremonies.take(&req.challenge)
    else {
        return Err(expired());
    };
    let passkey = state
        .webauthn
        .finish_passkey_registration(&req.credential, &reg)
        .map_err(registration_failed)?;
    let device = user_agent(&headers);
    let now = now_ms();

    let mut tx = state.db.begin().await?;
    if username_taken(&mut *tx, &account.username, None).await? {
        return Err(ApiError::conflict(
            "username_taken",
            format!("@{} was just taken.", account.username),
        ));
    }
    if account.first_admin {
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
    if let Some(invite_id) = &account.invite_id {
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
        "INSERT INTO users (id, username, display_name, time_zone, is_admin, webauthn_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&user_id)
    .bind(&account.username)
    .bind(&account.display_name)
    .bind(&account.time_zone)
    .bind(account.first_admin)
    .bind(&account.webauthn_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if let Some(invite_id) = &account.invite_id {
        sqlx::query("UPDATE invites SET used_by = ? WHERE id = ?")
            .bind(&user_id)
            .bind(invite_id)
            .execute(&mut *tx)
            .await?;
    }
    insert_passkey(
        &mut tx,
        &user_id,
        &clean_passkey_name(&req.passkey_name, &device),
        &passkey,
    )
    .await?;
    let codes = recovery::regenerate(&mut tx, &user_id).await?;
    let action = if account.first_admin {
        format!(
            "{} created the instance's first (admin) account",
            account.username
        )
    } else {
        format!("{} signed up", account.username)
    };
    audit::record(&mut *tx, Some((&user_id, &account.username)), action).await?;
    tx.commit().await?;

    if account.first_admin {
        state.setup_token.lock().unwrap().take();
        tracing::info!("admin account created; the setup link no longer works");
    }
    let token = session::create(&state.db, &user_id, &device, false, true).await?;
    Ok(with_cookie(&state, &token, RecoveryCodes { codes }))
}

pub async fn insert_passkey(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    name: &str,
    passkey: &webauthn_rs::prelude::Passkey,
) -> ApiResult<String> {
    let id = new_id();
    sqlx::query("INSERT INTO passkeys (id, user_id, name, cred_id, data, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(name)
        .bind(b64url(passkey.cred_id().as_ref()))
        .bind(serde_json::to_string(passkey)?)
        .bind(now_ms())
        .execute(&mut **tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                ApiError::conflict("passkey_exists", "That passkey is already registered.")
            }
            e => e.into(),
        })?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Sign-in

pub async fn login_start(State(state): State<AppState>) -> ApiResult<Json<CeremonyStart>> {
    let (mut rcr, auth) = state
        .webauthn
        .start_discoverable_authentication()
        .map_err(ApiError::internal)?;
    // A button press opens the normal prompt, not the autofill UI.
    rcr.mediation = None;
    let challenge = state.ceremonies.insert(Ceremony::Login { state: auth });
    Ok(Json(CeremonyStart {
        challenge,
        options: serde_json::to_value(&rcr)?,
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct AssertionFinish {
    pub challenge: String,
    #[ts(type = "any")]
    pub credential: PublicKeyCredential,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Ok_ {
    pub ok: bool,
}

pub fn ok() -> Json<Ok_> {
    Json(Ok_ { ok: true })
}

/// Loads a stored passkey by credential ID, with its owner.
async fn passkey_by_cred(
    db: &sqlx::SqlitePool,
    cred_id: &[u8],
) -> sqlx::Result<Option<(String, webauthn_rs::prelude::Passkey, User)>> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, data, user_id FROM passkeys WHERE cred_id = ?")
            .bind(b64url(cred_id))
            .fetch_optional(db)
            .await?;
    let Some((id, data, user_id)) = row else {
        return Ok(None);
    };
    let user: User = sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE id = ?"
    ))
    .bind(user_id)
    .fetch_one(db)
    .await?;
    let passkey = serde_json::from_str(&data).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    Ok(Some((id, passkey, user)))
}

/// Saves the updated signature counter and the last-used time.
async fn touch_passkey(
    db: &sqlx::SqlitePool,
    id: &str,
    passkey: &mut webauthn_rs::prelude::Passkey,
    result: &webauthn_rs::prelude::AuthenticationResult,
) -> ApiResult<()> {
    passkey.update_credential(result);
    sqlx::query("UPDATE passkeys SET data = ?, last_used_at = ? WHERE id = ?")
        .bind(serde_json::to_string(passkey)?)
        .bind(now_ms())
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

fn unknown_passkey() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNAUTHORIZED,
        "unknown_passkey",
        "That passkey isn't registered on this instance.",
    )
}

pub async fn login_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AssertionFinish>,
) -> ApiResult<Response> {
    let Some(Ceremony::Login { state: auth }) = state.ceremonies.take(&req.challenge) else {
        return Err(expired());
    };
    let (user_handle, cred_id) = state
        .webauthn
        .identify_discoverable_authentication(&req.credential)
        .map_err(|_| unknown_passkey())?;
    let (pk_id, mut passkey, user) = passkey_by_cred(&state.db, cred_id)
        .await?
        .ok_or_else(unknown_passkey)?;
    if user.webauthn_id != user_handle.to_string() {
        return Err(unknown_passkey());
    }
    let result = state
        .webauthn
        .finish_discoverable_authentication_v2(&req.credential, auth, &passkey)
        .map_err(|_| unknown_passkey())?;
    if user.suspended {
        return Err(ApiError::forbidden(
            "This account is suspended. Contact an admin.",
        ));
    }
    touch_passkey(&state.db, &pk_id, &mut passkey, &result).await?;
    let token = session::create(&state.db, &user.id, &user_agent(&headers), false, true).await?;
    Ok(with_cookie(&state, &token, Ok_ { ok: true }))
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
// Recovery codes and enrollment links

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RecoverySignIn {
    pub username: String,
    pub code: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RecoverySignedIn {
    pub codes_left: i64,
}

pub async fn recovery_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RecoverySignIn>,
) -> ApiResult<Response> {
    let wrong = || {
        ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "bad_recovery_code",
            "That code and username don't match an account. Check for typos; each code works only once.",
        )
    };
    let name = req.username.trim().to_ascii_lowercase();
    // Besides the per-IP limit, slow down guessing against one account.
    state.limits.check("recovery-user", &name, 5)?;
    let user: Option<User> = sqlx::query_as(concat!(
        "SELECT ",
        crate::user_columns!(),
        " FROM users WHERE username = ?"
    ))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    let user = user.ok_or_else(wrong)?;
    let mut tx = state.db.begin().await?;
    if !recovery::consume(&mut tx, &user.id, &req.code).await? {
        return Err(wrong());
    }
    if user.suspended {
        return Err(ApiError::forbidden(
            "This account is suspended. Contact an admin.",
        ));
    }
    sqlx::query("INSERT INTO recovery_events (user_id, at, message) VALUES (?, ?, ?)")
        .bind(&user.id)
        .bind(now_ms())
        .bind(format!(
            "Signed in with a recovery code on {}.",
            user_agent(&headers)
        ))
        .execute(&mut *tx)
        .await?;
    let left = recovery::remaining(&mut *tx, &user.id).await?;
    tx.commit().await?;
    let token = session::create(&state.db, &user.id, &user_agent(&headers), true, false).await?;
    Ok(with_cookie(
        &state,
        &token,
        RecoverySignedIn { codes_left: left },
    ))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct EnrollInfo {
    pub valid: bool,
    pub username: Option<String>,
}

async fn live_enroll_link(
    db: impl sqlx::SqliteExecutor<'_>,
    token: &str,
) -> sqlx::Result<Option<(String, String)>> {
    sqlx::query_as(
        "SELECT e.user_id, u.username FROM enroll_links e JOIN users u ON u.id = e.user_id
         WHERE e.token_hash = ? AND e.used_at IS NULL AND e.expires_at > ?",
    )
    .bind(sha256(token))
    .bind(now_ms())
    .fetch_optional(db)
    .await
}

pub async fn enroll_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<EnrollInfo>> {
    let link = live_enroll_link(&state.db, &token).await?;
    Ok(Json(EnrollInfo {
        valid: link.is_some(),
        username: link.map(|(_, u)| u),
    }))
}

/// Uses an admin-issued enrollment link: signs in with a restricted session
/// whose only option is adding a passkey.
pub async fn enroll_use(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> ApiResult<Response> {
    let mut tx = state.db.begin().await?;
    let (user_id, _) = live_enroll_link(&mut *tx, &token)
        .await?
        .ok_or_else(|| ApiError::forbidden("This link has expired or has already been used."))?;
    sqlx::query("UPDATE enroll_links SET used_at = ? WHERE token_hash = ?")
        .bind(now_ms())
        .bind(sha256(&token))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO recovery_events (user_id, at, message) VALUES (?, ?, ?)")
        .bind(&user_id)
        .bind(now_ms())
        .bind(format!(
            "An admin's one-time enrollment link was used on {}.",
            user_agent(&headers)
        ))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let token = session::create(&state.db, &user_id, &user_agent(&headers), true, false).await?;
    Ok(with_cookie(&state, &token, Ok_ { ok: true }))
}

/// How long admin-issued enrollment links work.
pub const ENROLL_LINK_TTL_MS: i64 = DAY_MS;

// ---------------------------------------------------------------------------
// Passkey confirmation for sensitive actions

async fn user_passkeys(
    db: &sqlx::SqlitePool,
    user_id: &str,
) -> ApiResult<Vec<(String, webauthn_rs::prelude::Passkey)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, data FROM passkeys WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(db)
            .await?;
    rows.into_iter()
        .map(|(id, d)| Ok((id, serde_json::from_str(&d)?)))
        .collect()
}

pub async fn reauth_start(
    State(state): State<AppState>,
    AnySession(s): AnySession,
) -> ApiResult<Json<CeremonyStart>> {
    let keys = user_passkeys(&state.db, &s.user.id).await?;
    if keys.is_empty() {
        return Err(ApiError::bad_request("no_passkeys", "Add a passkey first."));
    }
    let creds: Vec<_> = keys.into_iter().map(|(_, k)| k).collect();
    let (rcr, auth) = state
        .webauthn
        .start_passkey_authentication(&creds)
        .map_err(ApiError::internal)?;
    let challenge = state.ceremonies.insert(Ceremony::Reauth {
        user_id: s.user.id,
        state: auth,
    });
    Ok(Json(CeremonyStart {
        challenge,
        options: serde_json::to_value(&rcr)?,
    }))
}

pub async fn reauth_finish(
    State(state): State<AppState>,
    Authed(s): Authed,
    Json(req): Json<AssertionFinish>,
) -> ApiResult<Json<Ok_>> {
    let Some(Ceremony::Reauth {
        user_id,
        state: auth,
    }) = state.ceremonies.take(&req.challenge)
    else {
        return Err(expired());
    };
    if user_id != s.user.id {
        return Err(expired());
    }
    let result = state
        .webauthn
        .finish_passkey_authentication(&req.credential, &auth)
        .map_err(|_| unknown_passkey())?;
    for (id, mut key) in user_passkeys(&state.db, &s.user.id).await? {
        if key.cred_id() == result.cred_id() {
            touch_passkey(&state.db, &id, &mut key, &result).await?;
        }
    }
    sqlx::query("UPDATE sessions SET reauth_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(&s.id)
        .execute(&state.db)
        .await?;
    Ok(ok())
}

/// Starts adding a passkey to the signed-in account. Restricted sessions
/// (recovery) may do this without confirmation; others need a recent passkey.
pub async fn passkey_add_start(
    State(state): State<AppState>,
    AnySession(s): AnySession,
) -> ApiResult<Json<CeremonyStart>> {
    if !s.restricted {
        s.require_recent_passkey()?;
    }
    let existing: Vec<_> = user_passkeys(&state.db, &s.user.id)
        .await?
        .into_iter()
        .map(|(_, k)| k.cred_id().clone())
        .collect();
    let uuid = Uuid::parse_str(&s.user.webauthn_id).map_err(ApiError::internal)?;
    let (mut ccr, reg) = state
        .webauthn
        .start_passkey_registration(uuid, &s.user.username, &s.user.display_name, Some(existing))
        .map_err(ApiError::internal)?;
    require_resident_key(&mut ccr);
    let challenge = state.ceremonies.insert(Ceremony::AddPasskey {
        user_id: s.user.id,
        state: reg,
    });
    Ok(Json(CeremonyStart {
        challenge,
        options: serde_json::to_value(&ccr)?,
    }))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct PasskeyAddFinish {
    pub challenge: String,
    #[ts(type = "any")]
    pub credential: RegisterPublicKeyCredential,
    pub name: String,
}

pub async fn passkey_add_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    AnySession(s): AnySession,
    Json(req): Json<PasskeyAddFinish>,
) -> ApiResult<Json<Ok_>> {
    let Some(Ceremony::AddPasskey {
        user_id,
        state: reg,
    }) = state.ceremonies.take(&req.challenge)
    else {
        return Err(expired());
    };
    if user_id != s.user.id {
        return Err(expired());
    }
    let passkey = state
        .webauthn
        .finish_passkey_registration(&req.credential, &reg)
        .map_err(registration_failed)?;
    let mut tx = state.db.begin().await?;
    insert_passkey(
        &mut tx,
        &s.user.id,
        &clean_passkey_name(&req.name, &user_agent(&headers)),
        &passkey,
    )
    .await?;
    if s.restricted {
        // Recovery is complete: this session becomes a normal one.
        sqlx::query("UPDATE sessions SET restricted = 0, reauth_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&s.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(ok())
}
