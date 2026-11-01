//! Database backups: a consistent snapshot (`VACUUM INTO`) once a day into
//! `<data_dir>/backups`, keeping the newest `backup_keep`, plus the
//! `mau backup <path>` command. Cover images are plain files; back up the
//! `covers` directory with your usual tools.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use sqlx::SqlitePool;

use crate::config::Config;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Writes a consistent copy of the database to `path`, which must not exist.
pub async fn snapshot(db: &SqlitePool, path: &Path) -> Result<()> {
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    sqlx::query("VACUUM INTO ?")
        .bind(path.to_string_lossy().as_ref())
        .execute(db)
        .await
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn backup_dir(config: &Config) -> PathBuf {
    config.data_dir.join("backups")
}

/// Existing scheduled backups, oldest first.
fn existing(dir: &Path) -> Vec<(SystemTime, PathBuf)> {
    let mut list: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| {
                    let n = e.file_name();
                    let n = n.to_string_lossy();
                    n.starts_with("mau-") && n.ends_with(".db")
                })
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    list.sort();
    list
}

/// Takes a snapshot if the newest is a day old (or missing), then prunes.
pub async fn run_if_due(config: &Config, db: &SqlitePool) -> Result<Option<PathBuf>> {
    if config.backup_keep == 0 {
        return Ok(None);
    }
    let dir = backup_dir(config);
    let list = existing(&dir);
    let due = list
        .last()
        .is_none_or(|(t, _)| t.elapsed().unwrap_or(DAY) >= DAY);
    let mut made = None;
    if due {
        let name = format!("mau-{}.db", stamp());
        let path = dir.join(name);
        snapshot(db, &path).await?;
        tracing::info!("database backup written to {}", path.display());
        made = Some(path);
    }
    let list = existing(&dir);
    if list.len() > config.backup_keep {
        for (_, old) in &list[..list.len() - config.backup_keep] {
            if let Err(e) = std::fs::remove_file(old) {
                tracing::warn!("couldn't remove old backup {}: {e}", old.display());
            }
        }
    }
    Ok(made)
}

/// `YYYYMMDD-HHMMSS` in UTC.
fn stamp() -> String {
    let secs = crate::util::now_ms() / 1000;
    let date = crate::util::date_string(secs.div_euclid(86_400)).replace('-', "");
    let t = secs.rem_euclid(86_400);
    format!("{date}-{:02}{:02}{:02}", t / 3600, t % 3600 / 60, t % 60)
}

/// Checks hourly whether a backup is due.
pub fn schedule(config: std::sync::Arc<Config>, db: SqlitePool) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_if_due(&config, &db).await {
                tracing::error!("backup failed: {e:#}");
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshots_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            backup_keep: 2,
            ..Config::default()
        };
        let db = crate::db::open(&config.database_path()).await.unwrap();
        let first = run_if_due(&config, &db).await.unwrap();
        assert!(first.is_some());
        // Not due again within a day.
        assert!(run_if_due(&config, &db).await.unwrap().is_none());
        // Pretend older backups exist; only the newest two are kept.
        for name in ["mau-20200101-000000.db", "mau-20200102-000000.db"] {
            let p = backup_dir(&config).join(name);
            std::fs::write(&p, b"x").unwrap();
            let old = std::fs::File::options().write(true).open(&p).unwrap();
            old.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000))
                .unwrap();
        }
        run_if_due(&config, &db).await.unwrap();
        let left = existing(&backup_dir(&config));
        assert_eq!(left.len(), 2);
        assert_eq!(left.last().unwrap().1, first.unwrap());
        // The snapshot is a working database.
        let copy =
            sqlx::SqlitePool::connect(&format!("sqlite://{}", left.last().unwrap().1.display()))
                .await
                .unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&copy)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
