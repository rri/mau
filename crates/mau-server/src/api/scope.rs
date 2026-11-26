//! What a token may do. Scopes are `resource:action`; each `:write` also
//! allows its `:read`. Nothing about the account itself (password, two-factor,
//! sessions, recovery, tokens, the Google Books key), imports, invites, or
//! admin is reachable with any scope: those stay with a signed-in browser.

use std::collections::BTreeSet;

use serde::Serialize;
use ts_rs::TS;

/// Every scope, in the order screens list them, with its short label. Shown
/// under READ (seeing, searching) or READ & MODIFY (changing, removing).
pub const ALL: &[(&str, &str)] = &[
    ("profile:read", "Profile"),
    ("books:read", "Books and tags"),
    ("books:write", "Books"),
    ("books:delete", "Removing books"),
    ("tags:write", "Tags"),
    ("reading:read", "Reading progress"),
    ("reading:write", "Reading progress"),
    ("ratings:read", "Ratings"),
    ("ratings:write", "Ratings"),
    ("notes:read", "Private notes"),
    ("notes:write", "Private notes"),
    ("copies:read", "Copies and locations"),
    ("copies:write", "Copies and locations"),
    ("catalog:search", "Catalog search"),
    ("shares:read", "Sharing"),
    ("shares:write", "Sharing"),
];

/// `read` for scopes that only see (or search), `modify` for the rest.
pub fn group(scope: &str) -> &'static str {
    if scope.ends_with(":read") || scope == "catalog:search" {
        "read"
    } else {
        "modify"
    }
}

/// A scope with its description, for the consent screen and Settings.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScopeInfo {
    pub scope: String,
    pub label: String,
    /// read, or modify (read & modify)
    pub group: String,
}

/// A set of known scopes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scopes(BTreeSet<&'static str>);

impl Scopes {
    /// Parses a space-separated list. Unknown scopes are an error, naming the
    /// first one.
    pub fn parse(s: &str) -> Result<Scopes, String> {
        Self::from_iter_strs(s.split_ascii_whitespace())
    }

    pub fn from_iter_strs<'a>(it: impl IntoIterator<Item = &'a str>) -> Result<Scopes, String> {
        let mut set = BTreeSet::new();
        for s in it {
            let known = ALL
                .iter()
                .find(|(k, _)| *k == s)
                .ok_or_else(|| s.to_string())?;
            set.insert(known.0);
        }
        Ok(Scopes(set))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether this set allows `scope` (a `:write` allows its `:read`).
    pub fn allows(&self, scope: &str) -> bool {
        if self.0.contains(scope) {
            return true;
        }
        scope
            .strip_suffix(":read")
            .is_some_and(|res| self.0.iter().any(|s| s.strip_suffix(":write") == Some(res)))
    }

    /// Every scope in `self` is in `other`.
    pub fn is_subset(&self, other: &Scopes) -> bool {
        self.0.is_subset(&other.0)
    }

    pub fn union(&self, other: &Scopes) -> Scopes {
        Scopes(self.0.union(&other.0).copied().collect())
    }

    /// The scopes with their descriptions, in the order of [`ALL`].
    pub fn infos(&self) -> Vec<ScopeInfo> {
        ALL.iter()
            .filter(|(k, _)| self.0.contains(k))
            .map(|(k, l)| info(k, l))
            .collect()
    }

    pub fn list(&self) -> Vec<String> {
        self.infos().into_iter().map(|i| i.scope).collect()
    }
}

impl std::fmt::Display for Scopes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.list().join(" "))
    }
}

/// Every scope with its description.
pub fn all_infos() -> Vec<ScopeInfo> {
    ALL.iter().map(|(k, l)| info(k, l)).collect()
}

fn info(scope: &str, label: &str) -> ScopeInfo {
    ScopeInfo {
        scope: scope.to_string(),
        label: label.to_string(),
        group: group(scope).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_scopes_only() {
        let s = Scopes::parse("books:read  notes:write").unwrap();
        assert_eq!(s.to_string(), "books:read notes:write");
        assert_eq!(
            Scopes::parse("books:read admin").unwrap_err(),
            "admin".to_string()
        );
        assert!(Scopes::parse("").unwrap().is_empty());
    }

    #[test]
    fn scopes_are_grouped_by_what_they_allow() {
        assert_eq!(group("notes:read"), "read");
        assert_eq!(group("catalog:search"), "read");
        assert_eq!(group("notes:write"), "modify");
        assert_eq!(group("books:delete"), "modify");
        for (k, l) in ALL {
            assert!(l.len() <= 24, "{k}: keep labels short");
        }
    }

    #[test]
    fn write_allows_read() {
        let s = Scopes::parse("notes:write books:delete").unwrap();
        assert!(s.allows("notes:read"));
        assert!(s.allows("notes:write"));
        assert!(!s.allows("books:read"));
        assert!(!s.allows("ratings:read"));
    }

    #[test]
    fn nothing_about_the_account_or_admin_is_a_scope() {
        for (k, _) in ALL {
            assert!(
                ![
                    "admin", "account", "password", "session", "token", "import", "invite"
                ]
                .iter()
                .any(|w| k.contains(w)),
                "{k}"
            );
        }
    }
}
