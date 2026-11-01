//! The `mau` binary.

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
