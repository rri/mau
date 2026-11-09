//! Cover images: stored in `<data_dir>/covers` as JPEGs named
//! `<hash>-<size>.jpg`, where the hash is of the original image and the sizes
//! are s (120 px wide), m (300 px), and l (600 px). Covers from catalog
//! providers are downloaded and stored the same way, so pages never load
//! images from third parties.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::ImageReader;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::app::AppState;
use crate::auth::session::Authed;
use crate::error::{ApiError, ApiResult};

pub const MAX_BYTES: usize = 10 * 1024 * 1024;
const SIZES: [(&str, u32); 3] = [("s", 120), ("m", 300), ("l", 600)];
/// A small file can still claim huge dimensions ("decompression bomb"), so
/// decoding is capped in size and memory, and only two images decode at once.
const MAX_SIDE: u32 = 10_000;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;
static DECODING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("covers")
}

/// Decodes, resizes, and stores an image. Returns its hash.
pub async fn store(data_dir: &Path, bytes: Vec<u8>) -> ApiResult<String> {
    let dir = dir(data_dir);
    let _permit = DECODING.acquire().await.map_err(ApiError::internal)?;
    tokio::task::spawn_blocking(move || store_blocking(&dir, &bytes))
        .await
        .map_err(ApiError::internal)?
}

fn store_blocking(dir: &Path, bytes: &[u8]) -> ApiResult<String> {
    let hash: String = Sha256::digest(bytes)[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if dir.join(format!("{hash}-s.jpg")).exists() {
        return Ok(hash);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ApiError::internal)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let img = reader.decode().map_err(|_| {
        ApiError::bad_request(
            "bad_image",
            "That file isn't an image mau can read (JPEG, PNG, WebP, or GIF).",
        )
    })?;
    if img.width() < 8 || img.height() < 8 {
        return Err(ApiError::bad_request(
            "bad_image",
            "That image is too small to be a cover.",
        ));
    }
    std::fs::create_dir_all(dir).map_err(ApiError::internal)?;
    for (name, width) in SIZES {
        let resized = if img.width() > width {
            img.resize(width, width * 4, FilterType::Lanczos3)
        } else {
            img.clone()
        };
        let mut out = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
        resized
            .to_rgb8()
            .write_with_encoder(encoder)
            .map_err(ApiError::internal)?;
        let tmp = dir.join(format!("{hash}-{name}.jpg.tmp"));
        std::fs::write(&tmp, &out).map_err(ApiError::internal)?;
        std::fs::rename(&tmp, dir.join(format!("{hash}-{name}.jpg")))
            .map_err(ApiError::internal)?;
    }
    Ok(hash)
}

/// Hosts covers may be fetched from (catalog providers only).
const ALLOWED_HOSTS: &[&str] = &[
    "covers.openlibrary.org",
    "books.google.com",
    "books.googleusercontent.com",
    "assets.hardcover.app",
    "images.isbndb.com",
];

fn allowed(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
    })
}

/// Downloads a provider's cover. `None` if it's missing or unusable.
pub async fn fetch_bytes(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    if !allowed(url) {
        return None;
    }
    let res = http
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    if res.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
        return None;
    }
    // Read in chunks so a response without a length can't grow past the cap.
    let mut res = res;
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    // Open Library answers missing covers with a 1×1 placeholder.
    (bytes.len() > 1000).then_some(bytes)
}

pub async fn fetch_and_store(state: &AppState, url: &str) -> Option<String> {
    let bytes = fetch_bytes(&state.http, url).await?;
    store(&state.config.data_dir, bytes).await.ok()
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Uploaded {
    pub hash: String,
}

/// `POST /api/covers` with the image as the request body.
pub async fn upload(
    State(state): State<AppState>,
    Authed(_): Authed,
    body: Bytes,
) -> ApiResult<Json<Uploaded>> {
    if body.len() > MAX_BYTES {
        return Err(ApiError::bad_request(
            "too_big",
            "Choose an image under 10 MB.",
        ));
    }
    let hash = store(&state.config.data_dir, body.to_vec()).await?;
    Ok(Json(Uploaded { hash }))
}

/// `GET /covers/<hash>-<size>.jpg`. Names are content hashes, so responses
/// are cached forever.
pub async fn serve(State(state): State<AppState>, UrlPath(name): UrlPath<String>) -> Response {
    let valid = name.len() <= 40
        && name.ends_with(".jpg")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
    if !valid {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(dir(&state.config.data_dir).join(&name)).await {
        Ok(bytes) => {
            let mut res = Response::new(Body::from(bytes));
            let h = res.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
            h.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            res
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
pub struct ThumbQuery {
    u: String,
}

/// `GET /api/catalog/thumb?u=…`: passes a provider's cover through mau, so
/// search results can show covers without the browser contacting providers.
pub async fn thumb(
    State(state): State<AppState>,
    Authed(_): Authed,
    Query(q): Query<ThumbQuery>,
) -> Response {
    let Some(bytes) = fetch_bytes(&state.http, &q.u).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_three_sizes_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = image::RgbImage::new(800, 1200);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x % 255) as u8, (y % 255) as u8, 90]);
        }
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let hash = store_blocking(dir.path(), &png).unwrap();
        for (name, width) in SIZES {
            let saved = image::open(dir.path().join(format!("{hash}-{name}.jpg"))).unwrap();
            assert_eq!(saved.width(), width);
        }
        assert_eq!(store_blocking(dir.path(), &png).unwrap(), hash);
    }

    #[test]
    fn refuses_huge_dimensions() {
        // Wider than any cover could need.
        let img = image::GrayImage::new(MAX_SIDE + 1, 8);
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(png.len() < MAX_BYTES);
        let dir = tempfile::tempdir().unwrap();
        assert!(store_blocking(dir.path(), &png).is_err());
    }

    #[test]
    fn rejects_non_images() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store_blocking(dir.path(), b"not an image at all").is_err());
    }

    #[test]
    fn only_provider_hosts_are_fetched() {
        assert!(allowed("https://covers.openlibrary.org/b/id/1-L.jpg"));
        assert!(!allowed("http://covers.openlibrary.org/b/id/1-L.jpg"));
        assert!(!allowed("https://evil.example/x.jpg"));
        assert!(!allowed(
            "https://covers.openlibrary.org.evil.example/x.jpg"
        ));
    }
}
