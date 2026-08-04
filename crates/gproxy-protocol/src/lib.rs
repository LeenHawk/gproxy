//! GPROXY protocol types and endpoint metadata.
//!
//! The nested `protocol` module preserves the paths used before this code was
//! split out of the main crate. The root re-exports keep the public API compact
//! for downstream crates.

pub mod protocol;

pub use gproxy_protocol_macros::{WireBuilder, wire};
pub use protocol::*;

/// A required field was omitted while constructing a non-exhaustive wire
/// struct through its generated builder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WireBuildError {
    type_name: &'static str,
    field: &'static str,
}

impl WireBuildError {
    #[doc(hidden)]
    pub const fn missing(type_name: &'static str, field: &'static str) -> Self {
        Self { type_name, field }
    }

    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    pub const fn field(&self) -> &'static str {
        self.field
    }
}

impl std::fmt::Display for WireBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "missing required field `{}.{}`",
            self.type_name, self.field
        )
    }
}

impl std::error::Error for WireBuildError {}
