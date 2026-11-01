//! Fractional-index keys for manual ordering.
//!
//! A rank is a non-empty string of base-62 digits (`0-9A-Za-z`, in ASCII
//! order) that never ends in `0`. Ranks compare as plain strings, and there is
//! always room for a new rank between any two, so moving a row only rewrites
//! that row's rank. The web app has the same algorithm in
//! `web/src/lib/sync/rank.ts`; both are tested against the same cases.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: usize = 62;

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|&d| d == c).expect("rank contains only base-62 digits")
}

/// A rank strictly between `a` and `b`. `None` means "no bound" on that side.
///
/// # Panics
/// If `a >= b`, or either is not a valid rank.
pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    if let (Some(a), Some(b)) = (a, b) {
        assert!(a < b, "rank::between({a:?}, {b:?}): bounds out of order");
    }
    midpoint(a.unwrap_or("").as_bytes(), b.map(str::as_bytes))
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Skip the shared prefix (treating a missing digit in `a` as 0).
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let rest = midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..]));
            return format!("{}{}", std::str::from_utf8(&b[..n]).unwrap(), rest);
        }
    }
    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.map_or(BASE, |b| b.first().map_or(BASE, |&c| digit(c)));
    if db - da > 1 {
        let mid = (da + db).div_ceil(2);
        return (DIGITS[mid] as char).to_string();
    }
    if let Some(b) = b.filter(|b| b.len() > 1) {
        return (b[0] as char).to_string();
    }
    format!("{}{}", DIGITS[da] as char, midpoint(a.get(1..).unwrap_or(&[]), None))
}

/// `n` evenly spread ranks in order, for initial or imported lists.
pub fn spread(n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    let mut prev: Option<String> = None;
    for _ in 0..n {
        let next = between(prev.as_deref(), None);
        out.push(next.clone());
        prev = Some(next);
    }
    out
}

pub fn is_valid(rank: &str) -> bool {
    !rank.is_empty() && !rank.ends_with('0') && rank.bytes().all(|c| DIGITS.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        // Shared with web/src/lib/sync/rank.test.ts.
        assert_eq!(between(None, None), "V");
        assert_eq!(between(Some("V"), None), "l");
        assert_eq!(between(None, Some("V")), "G");
        assert_eq!(between(Some("V"), Some("W")), "VV");
        assert_eq!(between(Some("z"), None), "zV");
        assert_eq!(between(None, Some("1")), "0V");
    }

    #[test]
    fn always_strictly_between_and_valid() {
        let mut ranks = vec![between(None, None)];
        // Repeatedly insert at the front, the back, and in the middle.
        for i in 0..300 {
            let r = match i % 3 {
                0 => between(None, Some(&ranks[0])),
                1 => between(Some(ranks.last().unwrap()), None),
                _ => {
                    let m = ranks.len() / 2;
                    between(Some(&ranks[m - 1]), Some(&ranks[m]))
                }
            };
            assert!(is_valid(&r), "{r}");
            ranks.push(r);
            ranks.sort();
            ranks.dedup();
            assert_eq!(ranks.len(), i + 2, "duplicate rank");
        }
    }

    #[test]
    fn spread_is_sorted() {
        let s = spread(50);
        let mut sorted = s.clone();
        sorted.sort();
        assert_eq!(s, sorted);
        assert!(s.iter().all(|r| is_valid(r)));
    }
}
