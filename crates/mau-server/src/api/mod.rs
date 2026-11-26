//! Access for other programs: personal tokens, apps connected through OAuth
//! ("Sign in with mau"), and the public API at `/api/v1`.

pub mod client;
pub mod manage;
pub mod oauth;
pub mod scope;
pub mod token;
pub mod v1;

#[cfg(test)]
mod tests;
