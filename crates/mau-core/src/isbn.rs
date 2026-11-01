//! ISBN parsing: ISBN-10 and ISBN-13 are accepted and stored as ISBN-13.

/// Strips spaces and hyphens, validates the check digit, and returns the
/// ISBN-13. `None` if the input isn't a valid ISBN.
pub fn normalize(input: &str) -> Option<String> {
    let s: String = input.chars().filter(|c| !matches!(c, ' ' | '-')).collect::<String>().to_ascii_uppercase();
    match s.len() {
        13 if s.bytes().all(|b| b.is_ascii_digit()) && valid13(&s) => Some(s),
        10 if s[..9].bytes().all(|b| b.is_ascii_digit())
            && matches!(s.as_bytes()[9], b'0'..=b'9' | b'X')
            && valid10(&s) =>
        {
            Some(to13(&s))
        }
        _ => None,
    }
}

/// True if `input` looks like an ISBN attempt (digits with optional X,
/// hyphens, spaces), valid or not. Used to tell ISBN searches from text.
pub fn looks_like_isbn(input: &str) -> bool {
    let s: String = input.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
    (s.len() == 13 && s.bytes().all(|b| b.is_ascii_digit()))
        || (s.len() == 10
            && s[..9].bytes().all(|b| b.is_ascii_digit())
            && matches!(s.as_bytes()[9], b'0'..=b'9' | b'X' | b'x'))
}

fn valid13(s: &str) -> bool {
    let sum: u32 = s
        .bytes()
        .take(12)
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 1 { 3 } else { 1 })
        .sum();
    (10 - sum % 10) % 10 == u32::from(s.as_bytes()[12] - b'0')
}

fn valid10(s: &str) -> bool {
    let sum: u32 = s
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let v = if b == b'X' { 10 } else { u32::from(b - b'0') };
            v * (10 - i as u32)
        })
        .sum();
    sum % 11 == 0
}

fn to13(isbn10: &str) -> String {
    let base = format!("978{}", &isbn10[..9]);
    let sum: u32 = base
        .bytes()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 1 { 3 } else { 1 })
        .sum();
    format!("{base}{}", (10 - sum % 10) % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn13() {
        assert_eq!(normalize("978-0-306-40615-7").as_deref(), Some("9780306406157"));
        assert_eq!(normalize("9780306406158"), None);
    }

    #[test]
    fn isbn10_converts_to_13() {
        assert_eq!(normalize("0-306-40615-2").as_deref(), Some("9780306406157"));
        assert_eq!(normalize("080442957X").as_deref(), Some("9780804429573"));
        assert_eq!(normalize("0306406153"), None);
    }

    #[test]
    fn detection() {
        assert!(looks_like_isbn("978 0 306 40615 8"));
        assert!(looks_like_isbn("080442957x"));
        assert!(!looks_like_isbn("earthsea"));
        assert!(!looks_like_isbn("12345"));
    }
}
