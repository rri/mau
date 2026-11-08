//! Passwords: the rules for choosing one, hashing, and checking.

use std::collections::HashSet;
use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand::RngCore;

use crate::error::ApiError;

pub const MIN_CHARS: usize = 10;
pub const MAX_CHARS: usize = 256;

static COMMON: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("common-passwords.txt")
        .lines()
        .filter(|l| !l.starts_with("# "))
        .collect()
});

/// Checks a new password. Lengths count characters, not bytes.
pub fn validate(password: &str, username: &str) -> Result<(), ApiError> {
    let n = password.chars().count();
    let bad = |m: &str| Err(ApiError::bad_request("password", m));
    if n < MIN_CHARS {
        return bad("Use at least 10 characters.");
    }
    if n > MAX_CHARS {
        return bad("Use at most 256 characters.");
    }
    let lower = password.to_lowercase();
    if COMMON.contains(lower.as_str()) {
        return bad("That's one of the most common passwords. Choose another.");
    }
    if lower.trim() == username.to_lowercase() {
        return bad("Don't use your username as your password.");
    }
    Ok(())
}

fn hash_sync(password: &str) -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let salt = SaltString::encode_b64(&bytes).expect("16-byte salt");
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing")
        .to_string()
}

fn verify_sync(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// A hash of a password nobody knows, checked when the account doesn't exist
/// or has no password, so that answers take as long as for real accounts.
static DUMMY: LazyLock<String> = LazyLock::new(|| hash_sync("not a real password"));

pub async fn hash(password: &str) -> String {
    let p = password.to_string();
    tokio::task::spawn_blocking(move || hash_sync(&p))
        .await
        .expect("hashing task")
}

/// Whether `password` matches `phc`. With no hash, checks a dummy one and
/// answers no.
pub async fn verify(password: &str, phc: Option<&str>) -> bool {
    let p = password.to_string();
    let h = phc.map(str::to_string);
    tokio::task::spawn_blocking(move || match h {
        Some(h) => verify_sync(&p, &h),
        None => {
            verify_sync(&p, &DUMMY);
            false
        }
    })
    .await
    .expect("verify task")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert!(validate("short", "rri").is_err());
        assert!(validate(&"x".repeat(257), "rri").is_err());
        assert!(validate("Password123", "rri").is_err());
        assert!(validate("someone_long", "Someone_Long").is_err());
        assert!(validate("correct horse battery", "rri").is_ok());
        // Counts characters, not bytes.
        assert!(validate("ääääääääää", "rri").is_ok());
    }

    #[test]
    fn hash_and_verify() {
        let h = hash_sync("correct horse battery");
        assert!(verify_sync("correct horse battery", &h));
        assert!(!verify_sync("correct horse batterz", &h));
    }
}
