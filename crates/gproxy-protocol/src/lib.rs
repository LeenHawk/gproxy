//! GPROXY v3 protocol model.
//!
//! The operation taxonomy ([`operation`]) and the OperationSpec registry
//! ([`spec`], tables under `specs/`): every fact about an operation —
//! ingress paths, wire kinds, settle mode, affinity — declared once and
//! read by classification, channels, settlement, and console metadata.
//!
//! Public request, response, and stream-event models live under their wire
//! family and preserve unmodeled fields through each struct's flattened rest.
//!
//! Enums are exhaustive under the workspace-internal `exhaustive` feature
//! and `#[non_exhaustive]` otherwise; see Cargo.toml.

pub mod aws;
pub mod claude;
pub mod gemini;
pub mod openai;
pub mod operation;
mod path;
pub mod spec;
mod specs;

pub use gproxy_protocol_macros::{WireBuilder, wire};

#[cfg(test)]
mod tests;

pub use operation::{
    ContentGenerationKind, Operation, OperationGroup, OperationKey, OperationKeyError,
    OperationKind, WireFamily,
};
pub use path::{match_ingress, match_ingress_for, match_path, request_target};
pub use spec::{
    Affinity, Ingress, Matched, OperationSpec, PathPattern, Seg, SettleMode, StreamDetect,
    StreamFraming, default_framing, streaming_sibling,
};

pub fn registered_operations() -> impl Iterator<Item = Operation> {
    specs::REGISTRY.iter().map(|(operation, _)| *operation)
}

/// A required field was omitted while constructing an extensible wire struct.
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
