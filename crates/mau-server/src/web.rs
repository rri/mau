//! Serves the SvelteKit single-page app embedded in the binary.
//!
//! Files from `web/build` are compiled in (in debug builds rust-embed reads
//! them from disk instead, so `just web-build` is enough to see changes).
//! Unknown paths get the SPA shell (`200.html`) so client-side routes work.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

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

/// The served path of a built asset whose file name starts with `prefix`
/// (e.g. the hashed Instrument Sans font, for server-rendered pages).
pub fn asset_path(prefix: &str) -> Option<String> {
    Assets::iter()
        .find(|p| {
            p.starts_with("_app/immutable/assets/")
                && p.rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with(prefix))
        })
        .map(|p| format!("/{p}"))
}

/// The Content-Security-Policy for every response. Scripts may only come from
/// this site, plus the app shell's inline start-up script (allowed by its
/// hash) and the barcode decoder's WebAssembly. Styles may be inline (Svelte
/// sets style attributes); images may be data: URLs (generated QR codes).
pub fn content_security_policy() -> std::borrow::Cow<'static, str> {
    // Debug builds read web/build from disk, so a rebuilt shell must be
    // picked up without restarting; release builds embed it, so hash once.
    if cfg!(debug_assertions) {
        return build_csp().into();
    }
    static CSP: LazyLock<String> = LazyLock::new(build_csp);
    CSP.as_str().into()
}

fn build_csp() -> String {
    {
        let hashes: String = Assets::get(SPA_SHELL)
            .map(|f| inline_script_hashes(&String::from_utf8_lossy(&f.data)))
            .unwrap_or_default()
            .iter()
            .map(|h| format!(" 'sha256-{h}'"))
            .collect();
        format!(
            "default-src 'self'; script-src 'self'{hashes} 'wasm-unsafe-eval'; \
             style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; \
             connect-src 'self'; worker-src 'self'; manifest-src 'self'; object-src 'none'; \
             base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
        )
    }
}

/// Base64 SHA-256 hashes of the inline `<script>` elements in `html`.
fn inline_script_hashes(html: &str) -> Vec<String> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let tag = &after[..open_end];
        let body = &after[open_end + 1..];
        let Some(close) = body.find("</script>") else {
            break;
        };
        if !tag.contains(" src=") {
            let digest = Sha256::digest(&body.as_bytes()[..close]);
            out.push(base64::engine::general_purpose::STANDARD.encode(digest));
        }
        rest = &body[close..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_inline_scripts_only() {
        let html = r#"<script src="/a.js"></script><p>x</p><script>go()</script>"#;
        // printf 'go()' | openssl dgst -sha256 -binary | base64
        assert_eq!(
            inline_script_hashes(html),
            vec!["5KYv+PUboo5h+0+YAtGRPbwv5d/QxzHslP4YGnUaxRw=".to_string()]
        );
    }

    #[test]
    fn the_shell_script_is_allowed() {
        if Assets::get(SPA_SHELL).is_some() {
            assert!(content_security_policy().contains("'sha256-"));
        }
    }
}
