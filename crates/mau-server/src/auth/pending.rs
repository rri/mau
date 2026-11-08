//! Short-lived server memory for steps that span two requests: a sign-in
//! waiting for its authenticator code, and a two-factor setup waiting for its
//! first code. Entries last five minutes; a restart means starting over.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::util::{MINUTE_MS, now_ms, random_token};

const TTL_MS: i64 = 5 * MINUTE_MS;
/// Wrong codes allowed for one sign-in before it must start over.
pub const MAX_CODE_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone)]
pub enum Pending {
    /// The password was right; the authenticator code is still needed.
    SignIn { user_id: String, attempts: u32 },
    /// A new secret shown to the user, not yet confirmed with a code.
    TotpSetup { user_id: String, secret: String },
}

#[derive(Default)]
pub struct PendingStore {
    inner: Mutex<HashMap<String, (i64, Pending)>>,
}

impl PendingStore {
    /// Stores an entry and returns the ticket the client sends back.
    pub fn insert(&self, p: Pending) -> String {
        let id = random_token();
        let now = now_ms();
        let mut map = self.inner.lock().unwrap();
        map.retain(|_, (expires, _)| *expires > now);
        map.insert(id.clone(), (now + TTL_MS, p));
        id
    }

    /// Returns a live entry without removing it.
    pub fn get(&self, id: &str) -> Option<Pending> {
        let map = self.inner.lock().unwrap();
        map.get(id)
            .filter(|(expires, _)| *expires > now_ms())
            .map(|(_, p)| p.clone())
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }

    /// Counts a wrong code for a sign-in. Returns false (and forgets the
    /// sign-in) once too many were wrong.
    pub fn count_attempt(&self, id: &str) -> bool {
        let mut map = self.inner.lock().unwrap();
        let Some((_, Pending::SignIn { attempts, .. })) = map.get_mut(id) else {
            return false;
        };
        *attempts += 1;
        if *attempts >= MAX_CODE_ATTEMPTS {
            map.remove(id);
            return false;
        }
        true
    }
}
