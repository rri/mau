//! In-memory state for WebAuthn ceremonies in progress.
//!
//! Between "start" and "finish" the server must remember the challenge it
//! issued. These entries live for five minutes; a restart simply means the
//! user tries again.

use std::collections::HashMap;
use std::sync::Mutex;

use webauthn_rs::prelude::{DiscoverableAuthentication, PasskeyAuthentication, PasskeyRegistration};

use crate::util::{MINUTE_MS, now_ms, random_token};

const TTL_MS: i64 = 5 * MINUTE_MS;

/// Everything needed to create the account once the passkey is registered.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub username: String,
    pub display_name: String,
    pub time_zone: String,
    pub webauthn_id: String,
    pub invite_id: Option<String>,
    pub first_admin: bool,
}

pub enum Ceremony {
    Register { account: NewAccount, state: PasskeyRegistration },
    AddPasskey { user_id: String, state: PasskeyRegistration },
    Login { state: DiscoverableAuthentication },
    Reauth { user_id: String, state: PasskeyAuthentication },
}

#[derive(Default)]
pub struct Ceremonies {
    inner: Mutex<HashMap<String, (i64, Ceremony)>>,
}

impl Ceremonies {
    /// Stores a ceremony and returns the ID the client sends back on finish.
    pub fn insert(&self, ceremony: Ceremony) -> String {
        let id = random_token();
        let now = now_ms();
        let mut map = self.inner.lock().unwrap();
        map.retain(|_, (expires, _)| *expires > now);
        map.insert(id.clone(), (now + TTL_MS, ceremony));
        id
    }

    /// Removes and returns a ceremony if it exists and hasn't expired.
    /// Each ceremony can be finished only once.
    pub fn take(&self, id: &str) -> Option<Ceremony> {
        let mut map = self.inner.lock().unwrap();
        let (expires, ceremony) = map.remove(id)?;
        (expires > now_ms()).then_some(ceremony)
    }
}
