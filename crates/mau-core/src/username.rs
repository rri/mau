//! Username rules: 3–30 characters of a–z, 0–9, `_` and `-`, case-insensitive.

/// Names that would collide with routes or look official.
const RESERVED: &[&str] = &[
    "admin",
    "administrator",
    "api",
    "app",
    "assets",
    "auth",
    "catalog",
    "covers",
    "enroll",
    "help",
    "invite",
    "login",
    "logout",
    "mau",
    "me",
    "new",
    "reading",
    "root",
    "s",
    "settings",
    "setup",
    "signin",
    "signup",
    "static",
    "support",
    "system",
    "u",
    "user",
    "users",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    Length,
    Characters,
    Reserved,
}

impl Invalid {
    pub fn message(&self) -> &'static str {
        match self {
            Invalid::Length | Invalid::Characters => "Use 3–30 characters: a–z, 0–9, _ and -.",
            Invalid::Reserved => "That name is reserved.",
        }
    }
}

/// Returns the canonical (lowercase) username, or why it isn't allowed.
pub fn validate(input: &str) -> Result<String, Invalid> {
    let name = input.trim().to_ascii_lowercase();
    if !(3..=30).contains(&name.len()) {
        return Err(Invalid::Length);
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err(Invalid::Characters);
    }
    if RESERVED.contains(&name.as_str()) {
        return Err(Invalid::Reserved);
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(validate(" RRi "), Ok("rri".into()));
        assert_eq!(validate("ab"), Err(Invalid::Length));
        assert_eq!(validate("a".repeat(31).as_str()), Err(Invalid::Length));
        assert_eq!(validate("anne.marie"), Err(Invalid::Characters));
        assert_eq!(validate("zoë"), Err(Invalid::Characters));
        assert_eq!(validate("Admin"), Err(Invalid::Reserved));
        assert_eq!(validate("book_club-2"), Ok("book_club-2".into()));
    }
}
