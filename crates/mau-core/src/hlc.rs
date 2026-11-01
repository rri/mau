//! Hybrid logical clock timestamps.
//!
//! A timestamp is `"{ms:013}-{counter:04}-{node}"`: wall-clock milliseconds,
//! a counter that breaks ties within one millisecond, and the ID of the
//! device that made it. The fixed-width format makes plain string comparison
//! match timestamp order, which is how both SQLite and the browser compare
//! them. The web app has a matching implementation in `web/src/lib/sync/hlc.ts`.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hlc {
    pub ms: u64,
    pub counter: u16,
    pub node: String,
}

impl Hlc {
    pub const ZERO: &'static str = "0000000000000-0000-";
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:013}-{:04}-{}", self.ms, self.counter, self.node)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("invalid HLC timestamp: {0:?}")]
pub struct ParseError(String);

impl FromStr for Hlc {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseError(s.to_string());
        let (ms, rest) = s.split_once('-').ok_or_else(err)?;
        let (counter, node) = rest.split_once('-').ok_or_else(err)?;
        if ms.len() != 13 || counter.len() != 4 || node.is_empty() || node.len() > 32 {
            return Err(err());
        }
        if !node.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(err());
        }
        Ok(Hlc {
            ms: ms.parse().map_err(|_| err())?,
            counter: counter.parse().map_err(|_| err())?,
            node: node.to_string(),
        })
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.ms, self.counter, &self.node).cmp(&(other.ms, other.counter, &other.node))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Generates increasing timestamps for one node.
#[derive(Debug)]
pub struct Clock {
    node: String,
    last_ms: u64,
    counter: u16,
}

impl Clock {
    pub fn new(node: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            last_ms: 0,
            counter: 0,
        }
    }

    /// A timestamp later than every one this clock has issued or observed.
    pub fn now(&mut self, wall_ms: u64) -> Hlc {
        if wall_ms > self.last_ms {
            self.last_ms = wall_ms;
            self.counter = 0;
        } else {
            self.counter = self.counter.saturating_add(1);
        }
        Hlc {
            ms: self.last_ms,
            counter: self.counter,
            node: self.node.clone(),
        }
    }

    /// Moves the clock past a timestamp received from another node, so the
    /// next local timestamp sorts after it even if our wall clock is behind.
    pub fn observe(&mut self, remote: &Hlc) {
        if remote.ms > self.last_ms {
            self.last_ms = remote.ms;
            self.counter = remote.counter;
        } else if remote.ms == self.last_ms {
            self.counter = self.counter.max(remote.counter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_order_matches_timestamp_order() {
        let a = Hlc {
            ms: 999,
            counter: 5,
            node: "b".into(),
        };
        let b = Hlc {
            ms: 1000,
            counter: 0,
            node: "a".into(),
        };
        assert!(a < b);
        assert!(a.to_string() < b.to_string());
    }

    #[test]
    fn round_trips_through_string() {
        let h = Hlc {
            ms: 1_793_572_081_259,
            counter: 3,
            node: "dev1".into(),
        };
        assert_eq!(h.to_string(), "1793572081259-0003-dev1");
        assert_eq!(h.to_string().parse::<Hlc>().unwrap(), h);
    }

    #[test]
    fn rejects_malformed() {
        for s in [
            "",
            "123-0001-x",
            "1793572081259-1-x",
            "1793572081259-0001-",
            "1793572081259-0001-a-b",
        ] {
            assert!(s.parse::<Hlc>().is_err(), "{s}");
        }
    }

    #[test]
    fn clock_is_monotonic_when_wall_clock_stalls_or_goes_back() {
        let mut c = Clock::new("n");
        let a = c.now(100);
        let b = c.now(100);
        let d = c.now(50);
        assert!(a < b && b < d);
    }

    #[test]
    fn observing_a_later_remote_moves_the_clock_past_it() {
        let mut c = Clock::new("n");
        c.now(100);
        let remote = Hlc {
            ms: 500,
            counter: 7,
            node: "r".into(),
        };
        c.observe(&remote);
        assert!(c.now(200) > remote);
    }
}
