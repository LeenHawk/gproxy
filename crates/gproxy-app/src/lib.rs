//! The GPROXY v4 product layer: who is calling, what they may do, and the
//! operations that change that. It owns identity (users, gateway API keys,
//! organizations, teams, permissions, subscriptions, rate limits, the OAuth
//! issuer, audit), admission, and the typed admin/portal/issuer operations.
//!
//! What it deliberately does not own: an HTTP framework, a router, a runtime,
//! a CLI, and every engine concern (routing, credential selection, failover,
//! protocol conversion, settlement). Hosts adapt transports to the types here;
//! `gproxy-core` and `gproxy-sdk` execute. That is what keeps this crate
//! buildable for both a native server and `wasm32-unknown-unknown`.
//!
//! The planned request shape is `Caller` → `Admitted` → the sdk's `call()`:
//! authentication produces a caller, admission turns it into an allowed
//! provider set, an allowed credential set, a budget owner chain, a scope and
//! a session identity, and the sdk executes with exactly those. Only the
//! identity snapshot and the configuration type exist at this point; the
//! remaining modules are declared so later phases fill them in place.

mod error;
pub use error::AppError;

mod hex;

pub mod config;
pub use config::AppConfig;

pub mod snapshot;
pub use snapshot::{AppData, AppSnapshot};

pub mod admission;
pub mod audit;
pub mod auth;
pub mod call;
pub mod capture;
pub mod dto;
pub mod operations;
pub mod publication;
pub mod service;

pub type Result<T> = std::result::Result<T, AppError>;

/// Wall clock in milliseconds since the Unix epoch, saturating rather than
/// panicking on a clock the platform reports as before it. `web-time` gives
/// the same call `Date.now()` semantics inside a Worker isolate.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
