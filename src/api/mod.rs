//! Admin API DTOs — the single source of truth for the admin HTTP surface (§4).
//!
//! Pure serde + `http` types so the whole module compiles on every target
//! (native full, wasm edge, native minimal). The only native-gated piece is
//! the axum `IntoResponse` impl for [`error::ApiError`].

pub mod auth;
pub mod credentials;
pub mod error;
pub mod login;
pub mod routing;
pub mod tls_presets;
pub mod user_keys;
pub mod users;
