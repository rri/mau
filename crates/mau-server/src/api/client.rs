//! OAuth apps, known by their address. An app's `client_id` is its https://
//! address (or, while it's being built, an address on this computer); there's
//! no registration. What people are shown is the address's origin, so the
//! consent screen always says where the token will go.
//!
//! An app may publish a small JSON document at its `client_id` (the IETF
//! "client ID metadata document"): a display name, and redirect addresses
//! off its own origin (loopback or an app scheme, for native apps). Without
//! one, an app may only redirect to its own origin.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use crate::util::{HOUR_MS, now_ms};

/// A valid `client_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId {
    /// The normalized address, which is the stored ID.
    pub id: String,
    /// scheme://host[:port]
    pub origin: String,
    /// An app running on this computer (http://localhost or a loopback IP).
    pub local: bool,
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "[::1]"
}

/// Backslashes and control characters or spaces: URL parsers disagree about
/// these (`https://app.example\\@evil.example` is the app to some and evil to
/// others), so an address containing them is refused outright.
fn ambiguous(s: &str) -> bool {
    s.chars()
        .any(|c| c == '\\' || c.is_control() || c.is_whitespace())
}

fn origin_of(u: &Url) -> String {
    u.origin().ascii_serialization()
}

/// Checks a `client_id`: an https:// address on a domain name (not an IP
/// address), or http:// on this computer; no user name, password or
/// fragment; at most 300 characters.
pub fn parse_client_id(s: &str) -> Result<ClientId, &'static str> {
    if s.is_empty() || s.len() > 300 {
        return Err("The app's address (client_id) is missing or too long.");
    }
    if ambiguous(s) {
        return Err("The app's address (client_id) can't contain backslashes or spaces.");
    }
    let u = Url::parse(s).map_err(|_| "The app's address (client_id) isn't a valid URL.")?;
    if !u.username().is_empty() || u.password().is_some() || u.fragment().is_some() {
        return Err("The app's address (client_id) can't contain a user name or a #fragment.");
    }
    let host = u.host_str().unwrap_or_default();
    let local = match u.scheme() {
        "https" => match u.host() {
            Some(url::Host::Domain(d)) if !d.ends_with(".localhost") => d == "localhost",
            _ if is_loopback_host(host) => true,
            _ => {
                return Err(
                    "The app's address (client_id) must use a domain name, not an IP address.",
                );
            }
        },
        "http" if is_loopback_host(host) => true,
        _ => return Err("The app's address (client_id) must start with https://."),
    };
    Ok(ClientId {
        id: u.to_string(),
        origin: origin_of(&u),
        local,
    })
}

/// What an app's metadata document may say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub name: Option<String>,
    /// Extra redirect addresses (exactly as listed).
    pub redirect_uris: Vec<String>,
}

#[derive(Deserialize)]
struct RawMetadata {
    client_id: String,
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
}

/// Reads a metadata document. Its `client_id` must be the address it was
/// fetched from. Names are trimmed to 60 characters without control
/// characters.
pub fn parse_metadata(client: &ClientId, body: &[u8]) -> Option<Metadata> {
    let raw: RawMetadata = serde_json::from_slice(body).ok()?;
    if raw.client_id != client.id {
        return None;
    }
    let name = raw
        .client_name
        .map(|n| {
            n.chars()
                .filter(|c| !c.is_control())
                .take(60)
                .collect::<String>()
                .trim()
                .to_string()
        })
        .filter(|n| !n.is_empty());
    Some(Metadata {
        name,
        redirect_uris: raw.redirect_uris.into_iter().take(10).collect(),
    })
}

/// Whether `redirect` is an address the app may be sent back to:
/// - on the app's own origin, or
/// - listed in its metadata document, and either on a loopback address
///   (any port, as RFC 8252 asks) or an app's own scheme (`com.example.app:`).
///
/// Fragments are never allowed.
pub fn redirect_allowed(client: &ClientId, meta: Option<&Metadata>, redirect: &str) -> bool {
    if ambiguous(redirect) || redirect.len() > 1000 {
        return false;
    }
    let Ok(r) = Url::parse(redirect) else {
        return false;
    };
    if r.fragment().is_some() || !r.username().is_empty() || r.password().is_some() {
        return false;
    }
    if matches!(r.scheme(), "https" | "http") && origin_of(&r) == client.origin {
        return r.scheme() == "https" || client.local;
    }
    let Some(meta) = meta else { return false };
    meta.redirect_uris.iter().any(|listed| {
        let Ok(l) = Url::parse(listed) else {
            return false;
        };
        if l.fragment().is_some() {
            return false;
        }
        match l.scheme() {
            "http" if is_loopback_host(l.host_str().unwrap_or_default()) => {
                // Loopback: any port, the rest exactly as listed.
                r.scheme() == "http"
                    && r.host_str() == l.host_str()
                    && r.path() == l.path()
                    && r.query() == l.query()
            }
            "http" | "https" | "javascript" | "data" | "file" | "blob" | "about" => false,
            // An app's own scheme must look like a reversed domain name.
            s if s.contains('.') => listed == redirect,
            _ => false,
        }
    })
}

/// Fetches and caches metadata documents. Fetches are https only, to public
/// addresses only (so an app's address can't make the server reach its own
/// network), without redirects or proxies, 5 seconds, 16 KB at most.
pub struct MetadataFetcher {
    http: reqwest::Client,
    cache: Mutex<HashMap<String, (i64, Option<Metadata>)>>,
}

const CACHE_MS: i64 = 24 * HOUR_MS;
const MISS_CACHE_MS: i64 = 5 * crate::util::MINUTE_MS;
const CACHE_MAX: usize = 1000;
#[cfg(not(test))]
const MAX_BYTES: usize = 16 * 1024;

impl MetadataFetcher {
    pub fn new(user_agent: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .https_only(true)
            .dns_resolver(Arc::new(PublicOnly))
            .build()?;
        Ok(Self {
            http,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// The app's metadata, from the cache or its address. `None` if it has
    /// none (or it couldn't be read); apps on this computer are never fetched.
    pub async fn get(&self, client: &ClientId) -> Option<Metadata> {
        // Apps on this computer are never fetched, nor addresses on other
        // ports than https's own (the server isn't a port scanner).
        if client.local || Url::parse(&client.id).is_ok_and(|u| u.port().is_some()) {
            return None;
        }
        let now = now_ms();
        // A document that couldn't be read is tried again sooner.
        if let Some((at, m)) = self.cache.lock().unwrap().get(&client.id)
            && now - at < if m.is_some() { CACHE_MS } else { MISS_CACHE_MS }
        {
            return m.clone();
        }
        let m = self.fetch(client).await;
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_MAX {
            cache.retain(|_, (at, _)| now - *at < CACHE_MS);
            if cache.len() >= CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(client.id.clone(), (now, m.clone()));
        m
    }

    #[cfg(test)]
    async fn fetch(&self, _client: &ClientId) -> Option<Metadata> {
        let _ = &self.http;
        None
    }

    #[cfg(not(test))]
    async fn fetch(&self, client: &ClientId) -> Option<Metadata> {
        let mut res = self
            .http
            .get(&client.id)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .ok()?;
        if !res.status().is_success() {
            return None;
        }
        let mut body = Vec::new();
        while let Some(chunk) = res.chunk().await.ok()? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_BYTES {
                return None;
            }
        }
        parse_metadata(client, &body)
    }

    /// For tests: pretend an app published this document.
    #[cfg(test)]
    pub fn set(&self, client_id: &str, m: Option<Metadata>) {
        self.cache
            .lock()
            .unwrap()
            .insert(client_id.to_string(), (now_ms(), m));
    }
}

/// Resolves names to public addresses only.
struct PublicOnly;

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| is_public(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err("no public address".into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Whether an address is on the public internet: not loopback, private,
/// link-local, shared (CGNAT), documentation, benchmarking, multicast,
/// reserved, or unspecified.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x64 && s[1] == 0xff9b) // NAT64: an IPv4 address inside
                || (s[0] == 0x2002)) // 6to4: an IPv4 address inside
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ids() {
        let c = parse_client_id("https://stats.example.com").unwrap();
        assert_eq!(c.id, "https://stats.example.com/");
        assert_eq!(c.origin, "https://stats.example.com");
        assert!(!c.local);
        let c = parse_client_id("http://localhost:5173/app").unwrap();
        assert!(c.local);
        assert_eq!(c.origin, "http://localhost:5173");
        for bad in [
            "",
            "stats.example.com",
            "http://stats.example.com/",
            "https://1.2.3.4/",
            "https://[2001:db8::1]/",
            "https://u:p@example.com/",
            "https://example.com/#x",
            "ftp://example.com/",
            "javascript:alert(1)",
            "http://evil.localhost/",
        ] {
            assert!(parse_client_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn redirects_stay_on_the_apps_origin_unless_listed() {
        let c = parse_client_id("https://stats.example.com/").unwrap();
        assert!(redirect_allowed(&c, None, "https://stats.example.com/cb"));
        assert!(redirect_allowed(
            &c,
            None,
            "https://stats.example.com/cb?x=1"
        ));
        assert!(!redirect_allowed(
            &c,
            None,
            "https://stats.example.com/cb#f"
        ));
        assert!(!redirect_allowed(&c, None, "http://stats.example.com/cb"));
        assert!(!redirect_allowed(&c, None, "https://evil.example/cb"));
        assert!(!redirect_allowed(
            &c,
            None,
            "https://stats.example.com.evil.example/cb"
        ));
        assert!(!redirect_allowed(
            &c,
            None,
            "https://stats.example.com:444/cb"
        ));
        assert!(!redirect_allowed(&c, None, "http://127.0.0.1:9000/cb"));
        assert!(!redirect_allowed(&c, None, "com.example.stats:/cb"));
        // Parsers disagree about backslashes and control characters.
        for tricky in [
            "https://stats.example.com\\@evil.example/",
            "https://stats.example.com/cb\\..\\x",
            "https://stats.example.com/c b",
            "https://stats.example.com/\tcb",
            "https://stats.example.com/cb\n",
        ] {
            assert!(!redirect_allowed(&c, None, tricky), "{tricky:?}");
        }
        assert!(parse_client_id("https://stats.example.com\\@evil.example/").is_err());

        let meta = Metadata {
            name: None,
            redirect_uris: vec![
                "http://127.0.0.1/cb".into(),
                "com.example.stats:/cb".into(),
                "https://evil.example/cb".into(),
                "myapp:/cb".into(),
            ],
        };
        assert!(redirect_allowed(
            &c,
            Some(&meta),
            "http://127.0.0.1:9000/cb"
        ));
        assert!(!redirect_allowed(
            &c,
            Some(&meta),
            "http://127.0.0.1:9000/other"
        ));
        assert!(redirect_allowed(&c, Some(&meta), "com.example.stats:/cb"));
        // Listed, but another site: never.
        assert!(!redirect_allowed(
            &c,
            Some(&meta),
            "https://evil.example/cb"
        ));
        // A scheme that isn't a reversed domain name: never.
        assert!(!redirect_allowed(&c, Some(&meta), "myapp:/cb"));
    }

    #[test]
    fn look_alike_unicode_hosts_show_as_punycode() {
        // A Cyrillic "е" in place of the Latin "e".
        let c = parse_client_id("https://\u{0435}xample.com/").unwrap();
        assert!(c.origin.starts_with("https://xn--"), "{}", c.origin);
    }

    #[test]
    fn nested_blob_urls_are_refused_without_crashing() {
        // RUSTSEC-2026-0592: Url::origin() overflowed the stack on these.
        let deep = format!("{}https://stats.example.com/", "blob:".repeat(20_000));
        assert!(parse_client_id(&deep).is_err());
        let c = parse_client_id("https://stats.example.com/").unwrap();
        assert!(!redirect_allowed(&c, None, &deep));
        let meta = Metadata {
            name: None,
            redirect_uris: vec![deep.clone()],
        };
        assert!(!redirect_allowed(&c, Some(&meta), &deep));
    }

    #[test]
    fn local_apps_redirect_to_themselves_only() {
        let c = parse_client_id("http://localhost:5173/").unwrap();
        assert!(redirect_allowed(&c, None, "http://localhost:5173/cb"));
        assert!(!redirect_allowed(&c, None, "http://localhost:5174/cb"));
        assert!(!redirect_allowed(&c, None, "http://127.0.0.1:5173/cb"));
    }

    #[test]
    fn metadata_must_name_itself() {
        let c = parse_client_id("https://stats.example.com/").unwrap();
        let ok = br#"{"client_id":"https://stats.example.com/","client_name":"  Reading\u0007 Stats ","redirect_uris":["http://127.0.0.1/cb"]}"#;
        let m = parse_metadata(&c, ok).unwrap();
        assert_eq!(m.name.as_deref(), Some("Reading Stats"));
        assert_eq!(m.redirect_uris, vec!["http://127.0.0.1/cb"]);
        let other = br#"{"client_id":"https://other.example/","client_name":"X"}"#;
        assert!(parse_metadata(&c, other).is_none());
        assert!(parse_metadata(&c, b"not json").is_none());
    }

    #[test]
    fn only_public_addresses_are_fetched() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "64:ff9b::a00:1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "2606:2800:220:1::1"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }
}
