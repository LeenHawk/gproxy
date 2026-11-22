//! A thin GPROXY engine for serverless and edge platforms.
//!
//! Thin because it is single-tenant and has no database: one operator, one
//! access key, providers and their credentials handed in as configuration, and
//! all state behind one key-value interface with compare-and-exchange that the
//! host supplies ([`KeyValue`]). Channels, operations and protocol conversion
//! are the full gateway's own, from the stateless crates below this one.
//!
//! # Configuration
//!
//! ```json
//! {
//!   "access_key": "what clients present as their API key",
//!   "providers": [
//!     { "name": "openai", "channel": "openai", "credential": "sk-..." },
//!     { "name": "local", "channel": "custom", "base_url": "https://llm.example",
//!       "config": { "dialects": ["openai_chat"] }, "credential": "key" },
//!     { "name": "vertex", "channel": "vertex",
//!       "credential": { "kind": "service_account", "secret": { "...": "..." } } },
//!     { "name": "codex", "channel": "codex" }
//!   ]
//! }
//! ```
//!
//! `base_url` and `config` are the channel's own, as in the full gateway. A
//! provider without a `credential` signs in through `/lite/login/start` and
//! `/lite/login/complete`; `/lite/status` lists where each provider stands.
//!
//! # Staying signed in
//!
//! Nothing runs between requests, so the host calls [`Engine::renew_due`] on
//! a schedule (`POST /lite/renew` does the same): it renews what is about to
//! expire, so a refresh token is never left unused for longer than it lives.
//!
//! # Addressing
//!
//! `/{provider}/v1/...` sends to that provider with the model as its upstream
//! names it. Without the prefix the model field reads `provider/model`; with
//! a single provider configured the prefix may be left out.

// The stateless crates take `Arc<dyn …>` handles, which are not `Send` on
// wasm32, where nothing is.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

mod auth;
mod config;
mod credential;
mod engine;
mod exec;
mod login;
mod models;
mod renew;
mod resources;
mod store;
#[cfg(target_arch = "wasm32")]
mod wasm;
mod websocket;

pub use config::ConfigError;
pub use engine::Engine;
pub use renew::{Outcome, Renewal};
pub use store::{KeyValue, MemoryState};
pub use websocket::relay;

/// The largest request body the engine buffers.
pub(crate) const MAX_REQUEST_BYTES: u64 = 64 * 1024 * 1024;
