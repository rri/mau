//! The rule for concurrent progress changes: the latest position wins, and
//! page events from the losing branch stop counting as reading.
//!
//! Each position change carries `base`, the position clock its device saw
//! just before making it. A device's unsynced changes therefore form a chain
//! (each `base` is the previous change's clock), rooted at the last position
//! the device got from the server. The whole chain is judged by its newest
//! change (its tip):
//!
//! - tip older than the server's position: the chain loses, nothing applies;
//! - root equals the server's position: the chain simply continues it;
//! - otherwise another device moved on from the root meanwhile, and the chain
//!   is newer: that device's events after the root stop counting, then the
//!   chain applies.
//!
//! The web app mirrors this in `web/src/lib/sync/apply.ts`.

#[derive(Debug, PartialEq, Eq)]
pub enum Decision<'a> {
    Lose,
    Append,
    ReplaceAfter(&'a str),
}

/// Judges a chain from its root, its tip, and the server's current position clock.
pub fn decide<'a>(current: &str, root: &'a str, tip: &str) -> Decision<'a> {
    if tip <= current {
        Decision::Lose
    } else if root >= current {
        Decision::Append
    } else {
        Decision::ReplaceAfter(root)
    }
}

/// One progress change in a batch, in arrival order.
pub struct Change<'a> {
    pub reading: &'a str,
    pub base: &'a str,
    pub hlc: &'a str,
}

/// For each change: the root and tip of its chain, and whether it starts the chain.
#[derive(Debug, PartialEq, Eq)]
pub struct ChainInfo {
    /// Index of the chain within the batch.
    pub chain: usize,
    pub root: String,
    pub tip: String,
    pub first: bool,
}

/// Groups a batch's changes into chains per reading.
pub fn chains(changes: &[Change]) -> Vec<ChainInfo> {
    use std::collections::HashMap;
    // reading -> (chain index, last clock in chain)
    let mut open: HashMap<&str, (usize, &str)> = HashMap::new();
    let mut chain_of = Vec::with_capacity(changes.len());
    let mut roots: Vec<String> = Vec::new();
    let mut tips: Vec<String> = Vec::new();
    for c in changes {
        let continues = open
            .get(c.reading)
            .filter(|(_, last)| *last == c.base)
            .map(|(i, _)| *i);
        let (idx, first) = match continues {
            Some(i) => (i, false),
            None => {
                roots.push(c.base.to_string());
                tips.push(String::new());
                (roots.len() - 1, true)
            }
        };
        if c.hlc > tips[idx].as_str() {
            tips[idx] = c.hlc.to_string();
        }
        open.insert(c.reading, (idx, c.hlc));
        chain_of.push((idx, first));
    }
    chain_of
        .into_iter()
        .map(|(i, first)| ChainInfo {
            chain: i,
            root: roots[i].clone(),
            tip: tips[i].clone(),
            first,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "1000000000000-0000-srv";
    const A1: &str = "1000000000100-0000-aaa";
    const A2: &str = "1000000000200-0000-aaa";
    const B1: &str = "1000000000150-0000-bbb";
    const B2: &str = "1000000000300-0000-bbb";

    #[test]
    fn continuing_the_server_position_appends() {
        assert_eq!(decide(T0, T0, A2), Decision::Append);
    }

    #[test]
    fn newer_chain_from_an_older_root_replaces_the_other_branch() {
        // A synced A1, A2 from T0. B worked offline from T0; its tip B2 is newest.
        assert_eq!(decide(A2, T0, B2), Decision::ReplaceAfter(T0));
    }

    #[test]
    fn older_chain_loses() {
        // B synced first; A's tip A2 is older than B2.
        assert_eq!(decide(B2, T0, A2), Decision::Lose);
    }

    #[test]
    fn batch_is_split_into_chains() {
        let r = "r1";
        let batch = [
            Change {
                reading: r,
                base: T0,
                hlc: B1,
            },
            Change {
                reading: r,
                base: B1,
                hlc: B2,
            },
            Change {
                reading: "r2",
                base: T0,
                hlc: A1,
            },
        ];
        let c = chains(&batch);
        assert_eq!(
            c[0],
            ChainInfo {
                chain: 0,
                root: T0.into(),
                tip: B2.into(),
                first: true
            }
        );
        assert_eq!(
            c[1],
            ChainInfo {
                chain: 0,
                root: T0.into(),
                tip: B2.into(),
                first: false
            }
        );
        assert_eq!(
            c[2],
            ChainInfo {
                chain: 1,
                root: T0.into(),
                tip: A1.into(),
                first: true
            }
        );
    }
}
