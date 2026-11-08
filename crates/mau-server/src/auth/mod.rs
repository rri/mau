//! Accounts and authentication: passwords, authenticator codes, sessions,
//! recovery.

pub mod handlers;
pub mod manage;
pub mod password;
pub mod pending;
pub mod recovery;
pub mod session;
pub mod totp;

/// Account fields loaded with every authenticated request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub time_zone: String,
    pub is_admin: bool,
    pub suspended: bool,
    pub password_hash: Option<String>,
    pub totp_secret: Option<String>,
    pub totp_last_step: Option<i64>,
    pub google_books_key: Option<String>,
}

impl User {
    pub fn two_factor(&self) -> bool {
        self.totp_secret.is_some()
    }
}

/// Columns for [`User`], as a literal so it can be used in `concat!`.
#[macro_export]
macro_rules! user_columns {
    () => {
        "id, username, display_name, bio, time_zone, is_admin, suspended, password_hash, totp_secret, totp_last_step, google_books_key"
    };
}
