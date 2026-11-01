//! A user's library: books, readings, copies, tags. Filled in by milestone 3.

use sqlx::{Sqlite, Transaction};

/// Deletes everything a user owns that isn't removed by foreign-key cascades.
pub async fn delete_user_data(_tx: &mut Transaction<'_, Sqlite>, _user_id: &str) -> sqlx::Result<()> {
    Ok(())
}
