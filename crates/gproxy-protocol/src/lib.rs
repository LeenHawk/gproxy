//! GPROXY v3 protocol model.
//!
//! The operation taxonomy ([`operation`]) and the OperationSpec registry
//! ([`spec`], table in `specs.rs`): every fact about an operation —
//! ingress paths, wire kinds, settle mode, affinity — declared once and
//! read by classification, channels, settlement, and console metadata.
//!
//! Wire models (request/response types per family) land here as porting
//! proceeds, one family module at a time.
//!
//! Enums are exhaustive under the workspace-internal `exhaustive` feature
//! and `#[non_exhaustive]` otherwise; see Cargo.toml.

pub mod operation;
mod path;
pub mod spec;
mod specs;

#[cfg(test)]
mod tests;

pub use operation::{
    ContentGenerationKind, Operation, OperationGroup, OperationKey, OperationKind, WireFamily,
};
pub use path::{match_ingress, match_path};
pub use spec::{
    Affinity, Ingress, Matched, OperationSpec, PathPattern, Seg, SettleMode, StreamDetect,
    streaming_sibling,
};
