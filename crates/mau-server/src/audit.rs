//! The admin-visible audit log.

use sqlx::SqliteExecutor;

use crate::util::now_ms;

/// Records an action. `actor` is `(user_id, username)`, or `None` for the system.
pub async fn record(
    db: impl SqliteExecutor<'_>,
    actor: Option<(&str, &str)>,
    action: impl AsRef<str>,
) -> sqlx::Result<()> {
    let (id, name) = actor.map_or((None, "system"), |(i, n)| (Some(i), n));
    sqlx::query("INSERT INTO audit_log (at, actor_id, actor_name, action) VALUES (?, ?, ?, ?)")
        .bind(now_ms())
        .bind(id)
        .bind(name)
        .bind(action.as_ref())
        .execute(db)
        .await?;
    Ok(())
}
