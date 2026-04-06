//! Framework-independent routing primitives shared by gproxy components.

/// Request classification helpers.
pub mod classify;
/// Shared routing error types.
pub mod error;
/// Model alias data types.
pub mod model_alias;
/// Model extraction helpers.
pub mod model_extraction;
/// Model permission helpers.
pub mod permission;
/// Provider-prefix utilities for model identifiers.
pub mod provider_prefix;
/// Rate-limit rule matching helpers.
pub mod rate_limit;
/// Request sanitization helpers.
pub mod sanitize;

pub use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
