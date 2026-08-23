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

pub mod claude;
pub mod gemini;
pub mod openai;
pub mod operation;
mod path;
pub mod spec;
mod specs;

#[cfg(test)]
mod tests;

pub use operation::{
    ContentGenerationKind, Operation, OperationGroup, OperationKey, OperationKind, WireFamily,
};
pub use path::{match_ingress, match_ingress_for, match_path, request_target};
pub use spec::{
    Affinity, Ingress, Matched, OperationSpec, PathPattern, Seg, SettleMode, StreamDetect,
    StreamFraming, default_framing, streaming_sibling,
};
