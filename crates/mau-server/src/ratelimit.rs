//! Simple in-memory rate limits: at most N events per key per minute.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::ApiError;
use crate::util::{MINUTE_MS, now_ms};

#[derive(Default)]
pub struct RateLimits {
    inner: Mutex<HashMap<(String, String), Vec<i64>>>,
    calls: AtomicU64,
}

impl RateLimits {
    /// Records an event for `key` in `bucket`, or refuses if the last minute
    /// already had `per_minute` of them.
    pub fn check(&self, bucket: &str, key: &str, per_minute: usize) -> Result<(), ApiError> {
        let now = now_ms();
        let mut map = self.inner.lock().unwrap();
        if self.calls.fetch_add(1, Ordering::Relaxed) % 1000 == 999 {
            map.retain(|_, v| v.last().is_some_and(|t| now - t < MINUTE_MS));
        }
        let times = map
            .entry((bucket.to_string(), key.to_string()))
            .or_default();
        times.retain(|t| now - t < MINUTE_MS);
        if times.len() >= per_minute {
            return Err(ApiError::too_many_requests());
        }
        times.push(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_key() {
        let l = RateLimits::default();
        for _ in 0..3 {
            l.check("b", "k", 3).unwrap();
        }
        assert!(l.check("b", "k", 3).is_err());
        assert!(l.check("b", "other", 3).is_ok());
        assert!(l.check("c", "k", 3).is_ok());
    }
}
