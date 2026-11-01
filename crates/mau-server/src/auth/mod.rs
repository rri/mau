//! Accounts and authentication: passkeys, sessions, recovery.

pub mod ceremony;
pub mod handlers;
pub mod manage;
pub mod recovery;
pub mod session;

use anyhow::{Context, Result};
use webauthn_rs::prelude::{Url, Webauthn, WebauthnBuilder};

use crate::config::Config;

/// Builds the WebAuthn relying party from `public_url`. Passkeys are bound to
/// its host name.
pub fn build_webauthn(config: &Config) -> Result<Webauthn> {
    let origin = Url::parse(&config.public_url).context("public_url is not a valid URL")?;
    let rp_id = origin
        .host_str()
        .context("public_url has no host name")?
        .to_string();
    WebauthnBuilder::new(&rp_id, &origin)
        .context("invalid WebAuthn configuration")?
        .rp_name("mau")
        .build()
        .context("invalid WebAuthn configuration")
}

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
    pub webauthn_id: String,
}

/// Columns for [`User`], as a literal so it can be used in `concat!`.
#[macro_export]
macro_rules! user_columns {
    () => {
        "id, username, display_name, bio, time_zone, is_admin, suspended, webauthn_id"
    };
}
