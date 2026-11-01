//! Recovery codes: generation, hashing, and checking.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::{Rng, RngCore};
use sqlx::{Sqlite, Transaction};

use crate::util::now_ms;

pub const CODE_COUNT: usize = 10;
/// No 0/o, 1/l/i, to avoid misreading.
const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// A code like `k3v9p-q7d2m`.
pub fn generate_code() -> String {
    let mut rng = rand::rng();
    let mut s = String::with_capacity(11);
    for i in 0..10 {
        if i == 5 {
            s.push('-');
        }
        s.push(ALPHABET[rng.random_range(0..ALPHABET.len())] as char);
    }
    s
}

/// Lowercase, without spaces or the hyphen, so typing variations still match.
fn canonical(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

fn hash(code: &str) -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let salt = SaltString::encode_b64(&bytes).expect("16-byte salt");
    Argon2::default()
        .hash_password(canonical(code).as_bytes(), &salt)
        .expect("argon2 hashing")
        .to_string()
}

fn verify(code: &str, phc: &str) -> bool {
    PasswordHash::new(phc)
        .map(|h| Argon2::default().verify_password(canonical(code).as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// Replaces the user's codes with a fresh set and returns them in plain text.
pub async fn regenerate(tx: &mut Transaction<'_, Sqlite>, user_id: &str) -> sqlx::Result<Vec<String>> {
    let codes: Vec<String> = (0..CODE_COUNT).map(|_| generate_code()).collect();
    let hashes: Vec<String> = {
        let codes = codes.clone();
        tokio::task::spawn_blocking(move || codes.iter().map(|c| hash(c)).collect())
            .await
            .expect("hashing task")
    };
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?").bind(user_id).execute(&mut **tx).await?;
    for h in hashes {
        sqlx::query("INSERT INTO recovery_codes (user_id, hash) VALUES (?, ?)")
            .bind(user_id)
            .bind(h)
            .execute(&mut **tx)
            .await?;
    }
    Ok(codes)
}

/// Marks a matching unused code as used. Returns whether one matched.
pub async fn consume(tx: &mut Transaction<'_, Sqlite>, user_id: &str, code: &str) -> sqlx::Result<bool> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, hash FROM recovery_codes WHERE user_id = ? AND used_at IS NULL")
            .bind(user_id)
            .fetch_all(&mut **tx)
            .await?;
    let code = code.to_string();
    let matched = tokio::task::spawn_blocking(move || {
        rows.into_iter().find(|(_, h)| verify(&code, h)).map(|(id, _)| id)
    })
    .await
    .expect("verify task");
    let Some(id) = matched else { return Ok(false) };
    sqlx::query("UPDATE recovery_codes SET used_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(true)
}

pub async fn remaining(db: impl sqlx::SqliteExecutor<'_>, user_id: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM recovery_codes WHERE user_id = ? AND used_at IS NULL")
        .bind(user_id)
        .fetch_one(db)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_format() {
        let c = generate_code();
        assert_eq!(c.len(), 11);
        assert_eq!(&c[5..6], "-");
        assert!(c.bytes().filter(|&b| b != b'-').all(|b| ALPHABET.contains(&b)));
    }

    #[test]
    fn verify_tolerates_case_and_separators() {
        let h = hash("k3v9p-q7d2m");
        assert!(verify("K3V9P Q7D2M", &h));
        assert!(verify("k3v9pq7d2m", &h));
        assert!(!verify("k3v9p-q7d2n", &h));
    }
}
