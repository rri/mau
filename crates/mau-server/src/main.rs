//! The `mau` binary.

// Many queries read rows as tuples straight from sqlx; naming each tuple type
// would add indirection without making them clearer.
#![allow(clippy::type_complexity)]

#[cfg(test)]
mod access_tests;
mod admin;
mod app;
#[cfg(test)]
mod app_tests;
mod audit;
mod auth;
mod backup;
mod catalog;
mod client_ip;
mod config;
mod covers;
mod db;
mod error;
mod export;
mod importer;
mod library;
mod providers;
mod public;
mod quotes;
mod ratelimit;
mod seed;
mod settings;
mod shares;
mod util;
mod web;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

// musl's own allocator is slow when many threads allocate at once, so the
// static release build uses mimalloc.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "mau",
    version,
    about = "A self-hosted personal library and reading tracker"
)]
struct Cli {
    /// Config file (default: /etc/mau/mau.toml if it exists).
    #[arg(long, short, env = "MAU_CONFIG", global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server (the default).
    Serve,
    /// Write a consistent copy of the database to PATH (which must not exist).
    Backup { path: PathBuf },
    /// Print a one-time link (24 hours) for setting a new password. Using it
    /// also turns the account's two-factor off.
    ResetPassword {
        /// The account's username.
        username: String,
    },
    /// Development only: add the sample books from the design mocks to an account.
    Seed {
        /// Username to add the books to.
        #[arg(long)]
        user: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("MAU_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = config::Config::load(cli.config.as_deref())?;
    private_data_dir(&config.data_dir)?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config).await,
        Command::Backup { path } => {
            let db = db::open(&config.database_path()).await?;
            backup::snapshot(&db, &path).await?;
            println!(
                "Database copied to {}. Back up {} too.",
                path.display(),
                covers::dir(&config.data_dir).display()
            );
            Ok(())
        }
        Command::ResetPassword { username } => {
            let db = db::open(&config.database_path()).await?;
            let name = username.trim().to_ascii_lowercase();
            let mut tx = db.begin().await?;
            let id: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
                .bind(&name)
                .fetch_optional(&mut *tx)
                .await?;
            let id = id.with_context(|| format!("no account is named {name}"))?;
            let token = auth::handlers::issue_reset_link(&mut tx, &id, None).await?;
            audit::record(
                &mut *tx,
                None,
                format!("the server administrator issued a password reset link for {name}"),
            )
            .await?;
            tx.commit().await?;
            println!(
                "Open this link within 24 hours to set a new password for @{name}:\n\n  {}/reset/{token}\n\nUsing it also turns off two-factor for the account.",
                config.public_url
            );
            Ok(())
        }
        Command::Seed { user } => {
            let db = db::open(&config.database_path()).await?;
            seed::run(&config, &db, &user).await
        }
    }
}

async fn serve(config: config::Config) -> Result<()> {
    let db = db::open(&config.database_path()).await?;
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("listening on {}", config.listen))?;
    tracing::info!(listen = %config.listen, public_url = %config.public_url, data_dir = %config.data_dir.display(), "mau {} started", mau_core::VERSION);

    let state = app::AppState::new(db.clone(), config)?;
    announce_setup_link(&state).await?;
    backup::schedule(state.config.clone(), db.clone());
    axum::serve(
        listener,
        app::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    db.close().await;
    Ok(())
}

/// The data directory holds password hashes, two-factor secrets, API keys,
/// and backups, so only the account running mau may enter it.
fn private_data_dir(dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir)?.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .with_context(|| format!("making {} private", dir.display()))?;
            tracing::info!(
                "made {} private to this account (was {mode:o})",
                dir.display()
            );
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.ok() };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}

/// Until an admin exists, prints a one-time link for creating the admin
/// account. A new link is made on every start.
async fn announce_setup_link(state: &app::AppState) -> Result<()> {
    let has_admin: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_admin = 1)")
            .fetch_one(&state.db)
            .await?;
    if !has_admin {
        let token = util::random_token();
        let link = format!("{}/setup/{}", state.config.public_url, token);
        *state.setup_token.lock().unwrap() = Some(token);
        tracing::warn!("no admin account yet. Open this link to create it: {link}");
        eprintln!("\n  Create the admin account: {link}\n");
    }
    Ok(())
}
