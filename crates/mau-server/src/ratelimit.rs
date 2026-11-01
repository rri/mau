//! Simple in-memory rate limits: at most N events per key per minute.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

use crate::app::AppState;
use crate::client_ip::ClientIp;
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

/// Per-IP limits for sign-in, signup, and recovery endpoints (per minute).
fn rule(method: &Method, path: &str) -> Option<(&'static str, usize)> {
    let post = method == Method::POST;
    match path {
        "/auth/login/start" | "/auth/login/finish" if post => Some(("login", 20)),
        "/auth/recovery" if post => Some(("recovery", 5)),
        "/auth/register/start" | "/auth/register/finish" if post => Some(("register", 20)),
        "/auth/username-available" if post => Some(("username", 60)),
        "/auth/setup-check" if post => Some(("setup", 10)),
        p if p.starts_with("/auth/invite/") => Some(("invite", 20)),
        p if p.starts_with("/auth/enroll/") => Some(("enroll", 10)),
        _ => None,
    }
}

/// Middleware on the API router (paths here are relative to `/api`).
pub async fn layer(
    State(state): State<AppState>,
    ip: ClientIp,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if let Some((bucket, per_minute)) = rule(req.method(), req.uri().path()) {
        state.limits.check(bucket, &ip.key(), per_minute)?;
    }
    Ok(next.run(req).await)
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
