//! Domain services and in-memory state for the gproxy application layer.
//!
//! This crate extracts the domain logic from the monolithic `AppState` into
//! focused service structs, each owning a coherent slice of runtime state.

pub mod config;
pub mod file;
pub mod identity;
pub mod policy;
pub mod quota;
pub mod routing;
pub mod types;

pub use config::ConfigService;
pub use file::FileService;
pub use identity::IdentityService;
pub use policy::PolicyService;
pub use quota::QuotaService;
pub use routing::RoutingService;
pub use types::*;
