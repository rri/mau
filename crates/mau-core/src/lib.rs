//! Domain logic for mau, independent of HTTP and storage.

pub mod hlc;
pub mod import;
pub mod isbn;
pub mod progress;
pub mod rank;

/// The mau version, shared by the server and the API.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod username;
