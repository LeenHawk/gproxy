//! Framework-independent routing helpers (migrated from `gproxy-routing`).

pub mod classify;
pub mod error;
pub mod headers;
pub mod model_alias;
pub mod model_extraction;
pub mod permission;
pub mod provider_prefix;
pub mod rate_limit;

pub use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
