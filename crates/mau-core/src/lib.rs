//! Domain logic for mau, independent of HTTP and storage.

/// The mau version, shared by the server and the API.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
