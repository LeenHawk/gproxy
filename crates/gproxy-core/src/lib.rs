//! Domain services and in-memory state for the gproxy application layer.
//!
//! This crate extracts the domain logic from the monolithic `AppState` into
//! focused service structs, each owning a coherent slice of runtime state.

pub mod config;
pub mod dispatch;
pub mod file;
pub mod identity;
pub mod policy;
pub mod quota;
pub mod routing;
pub mod types;

/// Redis-backed implementations of backend traits for multi-instance deployments.
/// Enable with `features = ["redis"]`.
#[cfg(feature = "redis")]
pub mod redis_backend;

pub use config::ConfigService;
pub use file::FileService;
pub use identity::IdentityService;
pub use policy::PolicyService;
pub use quota::QuotaService;
pub use routing::RoutingService;
pub use types::*;
