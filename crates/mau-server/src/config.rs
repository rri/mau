//! Server configuration: a TOML file with `MAU_*` environment overrides.
//!
//! Only settings needed before the database is open live here. Everything an
//! admin can change at runtime (signup mode, providers, ...) is stored in the
//! database instead.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Where mau looks for a config file when none is given.
pub const DEFAULT_CONFIG_PATH: &str = "/etc/mau/mau.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Address to listen on for plain HTTP. HTTPS is the reverse proxy's job.
    pub listen: SocketAddr,
    /// The public address people use, e.g. `https://books.example.com`.
    /// Used in links (setup, invites, resets) and the authenticator-app label.
    pub public_url: String,
    /// Directory holding the database, covers, and backups.
    pub data_dir: PathBuf,
    /// Proxies whose `X-Forwarded-*` headers are trusted.
    pub trusted_proxies: Vec<IpAddr>,
    /// Daily database snapshots to keep in `<data_dir>/backups` (0 turns them off).
    pub backup_keep: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8370)),
            public_url: "http://localhost:8370".into(),
            data_dir: PathBuf::from("data"),
            trusted_proxies: vec![IpAddr::from([127, 0, 0, 1]), "::1".parse().unwrap()],
            backup_keep: 7,
        }
    }
}

impl Config {
    /// Loads the config: an explicit file if given, else the default path if it
    /// exists, else built-in defaults. Environment variables override the file.
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let mut config = match explicit {
            Some(path) => Self::from_file(path)?,
            None if Path::new(DEFAULT_CONFIG_PATH).exists() => {
                Self::from_file(Path::new(DEFAULT_CONFIG_PATH))?
            }
            None => Self::default(),
        };
        config.apply_env(|key| std::env::var(key).ok())?;
        config.validate()?;
        Ok(config)
    }

    fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))
    }

    fn apply_env(&mut self, get: impl Fn(&str) -> Option<String>) -> Result<()> {
        if let Some(v) = get("MAU_LISTEN") {
            self.listen = v
                .parse()
                .context("MAU_LISTEN must be an address like 127.0.0.1:8370")?;
        }
        if let Some(v) = get("MAU_PUBLIC_URL") {
            self.public_url = v;
        }
        if let Some(v) = get("MAU_DATA_DIR") {
            self.data_dir = PathBuf::from(v);
        }
        if let Some(v) = get("MAU_BACKUP_KEEP") {
            self.backup_keep = v.parse().context("MAU_BACKUP_KEEP must be a number")?;
        }
        if let Some(v) = get("MAU_TRUSTED_PROXIES") {
            self.trusted_proxies = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.parse()
                        .with_context(|| format!("MAU_TRUSTED_PROXIES: bad address {s:?}"))
                })
                .collect::<Result<_>>()?;
        }
        Ok(())
    }

    fn validate(&mut self) -> Result<()> {
        let url = self.public_url.trim_end_matches('/');
        if !(url.starts_with("https://") || url.starts_with("http://localhost")) {
            bail!(
                "public_url must start with https:// (passwords and session cookies need HTTPS), or be http://localhost for development"
            );
        }
        self.public_url = url.to_string();
        Ok(())
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("mau.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn env_overrides_file_values() {
        let mut config: Config = toml::from_str(
            r#"
            listen = "0.0.0.0:9000"
            public_url = "https://books.example.com/"
            "#,
        )
        .unwrap();
        let env = HashMap::from([
            ("MAU_LISTEN", "127.0.0.1:9100"),
            ("MAU_TRUSTED_PROXIES", "10.0.0.1, ::1"),
        ]);
        config
            .apply_env(|k| env.get(k).map(|v| v.to_string()))
            .unwrap();
        config.validate().unwrap();
        assert_eq!(config.listen.port(), 9100);
        assert_eq!(config.public_url, "https://books.example.com");
        assert_eq!(config.trusted_proxies.len(), 2);
    }

    #[test]
    fn rejects_plain_http_public_url() {
        let mut config = Config {
            public_url: "http://books.example.com".into(),
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<Config>("lissten = \"x\"").is_err());
    }
}
