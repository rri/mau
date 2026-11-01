//! Router and shared application state.

use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{Method, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;
use webauthn_rs::prelude::Webauthn;

use crate::auth::ceremony::Ceremonies;
use crate::auth::{handlers as auth, manage};
use crate::config::Config;
use crate::error::ApiError;
use crate::ratelimit::RateLimits;
use crate::web;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Ceremonies>,
    /// One-time link for creating the first admin; `None` once an admin exists.
    pub setup_token: Arc<Mutex<Option<String>>>,
    /// Outgoing HTTP (catalog providers, cover downloads).
    pub http: reqwest::Client,
    pub limits: Arc<RateLimits>,
}

impl AppState {
    pub fn new(db: SqlitePool, config: Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(format!(
                "mau/{} (+{})",
                mau_core::VERSION,
                config.public_url
            ))
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            limits: Arc::new(RateLimits::default()),
            webauthn: Arc::new(crate::auth::build_webauthn(&config)?),
            db,
            config: Arc::new(config),
            ceremonies: Arc::new(Ceremonies::default()),
            setup_token: Arc::new(Mutex::new(None)),
        })
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        // Sign-up, sign-in, recovery
        .route("/auth/status", get(auth::status))
        .route("/auth/username-available", post(auth::username_available))
        .route("/auth/invite/{token}", get(auth::invite_info))
        .route("/auth/setup-check", post(auth::setup_check))
        .route("/auth/register/start", post(auth::register_start))
        .route("/auth/register/finish", post(auth::register_finish))
        .route("/auth/login/start", post(auth::login_start))
        .route("/auth/login/finish", post(auth::login_finish))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/recovery", post(auth::recovery_sign_in))
        .route(
            "/auth/enroll/{token}",
            get(auth::enroll_info).post(auth::enroll_use),
        )
        .route("/auth/reauth/start", post(auth::reauth_start))
        .route("/auth/reauth/finish", post(auth::reauth_finish))
        // Account settings
        .route(
            "/account",
            patch(manage::update_account).delete(manage::delete_account),
        )
        .route("/passkeys", get(manage::list_passkeys))
        .route("/passkeys/start", post(auth::passkey_add_start))
        .route("/passkeys/finish", post(auth::passkey_add_finish))
        .route(
            "/passkeys/{id}",
            patch(manage::rename_passkey).delete(manage::delete_passkey),
        )
        .route("/sessions", get(manage::list_sessions))
        .route(
            "/sessions/revoke-others",
            post(manage::revoke_other_sessions),
        )
        .route("/sessions/{id}", delete(manage::revoke_session))
        .route("/recovery-codes", get(manage::recovery_status))
        .route("/recovery-codes/regenerate", post(manage::regenerate_codes))
        // Library sync
        .route("/sync", post(crate::library::sync))
        // Catalog and covers
        .route("/catalog/search", get(crate::catalog::search))
        .route("/catalog/editions", get(crate::catalog::editions))
        .route("/catalog/ensure", post(crate::catalog::ensure))
        .route("/catalog/similar", get(crate::catalog::similar))
        .route("/catalog/thumb", get(crate::covers::thumb))
        .route("/catalog/works/{id}", get(crate::catalog::work))
        .route("/catalog/works/{id}/suggest", post(crate::catalog::suggest))
        .route(
            "/covers",
            post(crate::covers::upload).layer(axum::extract::DefaultBodyLimit::max(
                crate::covers::MAX_BYTES,
            )),
        )
        .fallback(api_not_found)
        .layer(middleware::from_fn_with_state(state.clone(), check_origin));
    Router::new()
        .nest("/api", api)
        .route("/covers/{name}", get(crate::covers::serve))
        .fallback(web::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Rejects state-changing API requests from other sites. SameSite cookies
/// already block most of these; this also covers older browsers.
async fn check_origin(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(state.config.public_url.as_str()) {
            return Err(ApiError::forbidden(
                "Request origin doesn't match this site's address.",
            ));
        }
    }
    Ok(next.run(req).await)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: mau_core::VERSION,
    })
}

async fn api_not_found() -> ApiError {
    ApiError::not_found("No such API endpoint.")
}
