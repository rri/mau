//! Instance settings that admins change at runtime, stored as JSON values.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SignupMode {
    Open,
    Invite,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum InviteWho {
    Admins,
    All,
}

pub async fn get<T: DeserializeOwned>(db: impl SqliteExecutor<'_>, key: &str) -> sqlx::Result<Option<T>> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await?;
    Ok(raw.and_then(|v| serde_json::from_str(&v).ok()))
}

pub async fn set<T: Serialize>(db: impl SqliteExecutor<'_>, key: &str, value: &T) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO instance_settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(serde_json::to_string(value).expect("settings serialize"))
    .execute(db)
    .await?;
    Ok(())
}

pub async fn signup_mode(db: impl SqliteExecutor<'_>) -> sqlx::Result<SignupMode> {
    Ok(get(db, "signup_mode").await?.unwrap_or(SignupMode::Invite))
}

pub async fn invite_who(db: impl SqliteExecutor<'_>) -> sqlx::Result<InviteWho> {
    Ok(get(db, "invite_who").await?.unwrap_or(InviteWho::Admins))
}

/// Pending invites each non-admin user may have when everyone can invite.
pub async fn invite_limit(db: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    Ok(get(db, "invite_limit").await?.unwrap_or(5))
}
