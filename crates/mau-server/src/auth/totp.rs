//! Authenticator-app codes (TOTP, RFC 6238): 6 digits, 30-second steps,
//! HMAC-SHA1, which is what every common app supports.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use rand::RngCore;
use sha1::Sha1;

const STEP_SECS: i64 = 30;
/// Codes from one step before or after are accepted too, for clock drift.
const DRIFT_STEPS: i64 = 1;

/// A new 160-bit secret, base32 without padding.
pub fn new_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut bytes);
    BASE32_NOPAD.encode(&bytes)
}

fn code_at(key: &[u8], step: i64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(&step.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let offset = (h[h.len() - 1] & 0x0f) as usize;
    let n = u32::from_be_bytes([h[offset], h[offset + 1], h[offset + 2], h[offset + 3]]);
    (n & 0x7fff_ffff) % 1_000_000
}

/// Digits only, so "123 456" and "123-456" work.
fn digits(code: &str) -> Option<u32> {
    let d: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    (d.len() == 6 && d.bytes().all(|b| b.is_ascii_digit()))
        .then(|| d.parse().ok())
        .flatten()
}

/// Whether `code` looks like an authenticator code (rather than a recovery code).
pub fn looks_like_code(code: &str) -> bool {
    digits(code).is_some()
}

/// Checks `code` at `now_ms`. Returns the matched step, which must be
/// greater than `last_step` so a code can't be used twice.
pub fn verify(secret: &str, code: &str, now_ms: i64, last_step: Option<i64>) -> Option<i64> {
    let key = BASE32_NOPAD.decode(secret.as_bytes()).ok()?;
    let want = digits(code)?;
    let now = now_ms.div_euclid(1000 * STEP_SECS);
    (now - DRIFT_STEPS..=now + DRIFT_STEPS)
        .filter(|s| last_step.is_none_or(|l| *s > l))
        .find(|s| code_at(&key, *s) == want)
}

/// The `otpauth://` address apps read from the QR code.
pub fn uri(secret: &str, host: &str, username: &str) -> String {
    let enc = |s: &str| -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    let issuer = format!("mau ({host})");
    format!(
        "otpauth://totp/{}:{}?secret={secret}&issuer={}&algorithm=SHA1&digits=6&period=30",
        enc(&issuer),
        enc(username),
        enc(&issuer)
    )
}

/// The QR code for `uri` as an SVG document.
pub fn qr_svg(uri: &str) -> String {
    qrcode::QrCode::new(uri.as_bytes())
        .expect("otpauth URI fits in a QR code")
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 appendix B, SHA-1 key "12345678901234567890" (last 6 digits).
    #[test]
    fn rfc_vectors() {
        let key = b"12345678901234567890";
        for (t, want) in [
            (59i64, 287082u32),
            (1111111109, 81804),
            (1111111111, 50471),
            (1234567890, 5924),
            (2000000000, 279037),
        ] {
            assert_eq!(code_at(key, t / 30), want, "t={t}");
        }
    }

    #[test]
    fn verify_accepts_drift_and_refuses_replays() {
        let secret = BASE32_NOPAD.encode(b"12345678901234567890");
        let now = 59_000;
        assert_eq!(verify(&secret, "287 082", now, None), Some(1));
        // One step later still works.
        assert_eq!(verify(&secret, "287082", now + 30_000, None), Some(1));
        // Two steps later doesn't.
        assert_eq!(verify(&secret, "287082", now + 60_000, None), None);
        // Already used.
        assert_eq!(verify(&secret, "287082", now, Some(1)), None);
        assert_eq!(verify(&secret, "28708", now, None), None);
    }

    #[test]
    fn uri_is_escaped() {
        let u = uri("ABC", "books.example.com", "rri");
        assert_eq!(
            u,
            "otpauth://totp/mau%20%28books.example.com%29:rri?secret=ABC&issuer=mau%20%28books.example.com%29&algorithm=SHA1&digits=6&period=30"
        );
        assert!(qr_svg(&u).starts_with("<?xml"));
    }
}
