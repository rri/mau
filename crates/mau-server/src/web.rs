//! Serves the SvelteKit single-page app embedded in the binary.
//!
//! Files from `web/build` are compiled in (in debug builds rust-embed reads
//! them from disk instead, so `just web-build` is enough to see changes).
//! Unknown paths get the SPA shell (`200.html`) so client-side routes work.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/build"]
struct Assets;

const SPA_SHELL: &str = "200.html";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = (!path.is_empty()).then(|| Assets::get(path)).flatten() {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // SvelteKit puts content-hashed files under _app/immutable.
        let cache = if path.starts_with("_app/immutable/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return response(file.data.into_owned(), mime.as_ref(), cache);
    }
    match Assets::get(SPA_SHELL) {
        Some(file) => response(
            file.data.into_owned(),
            "text/html; charset=utf-8",
            "no-cache",
        ),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The web app isn't built. Run `just web-build`.",
        )
            .into_response(),
    }
}

fn response(body: Vec<u8>, content_type: &str, cache: &'static str) -> Response {
    let mut res = Response::new(Body::from(body));
    let headers = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res
}
