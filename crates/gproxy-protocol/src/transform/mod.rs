//! Direct vendor-wire conversions and the identity rules they share.
//!
//! A conversion only reads declared source fields and builds declared target
//! fields. Unknown `rest` members are neither conversion input nor output.
//! Conversion state and diagnostics are separate from vendor wire payloads.

pub mod count_tokens;
pub mod compact;
pub mod embeddings;
mod error;
pub mod files;
pub mod generate;
pub mod identity;
pub mod memory;
pub mod models;

pub use error::{Diagnostic, DiagnosticKind, Report, TransformError, TransformErrorKind};

/// A typed target value and out-of-band field diagnostics. This wrapper never
/// changes or normalizes the payload type into a common content model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted<T> {
    pub value: T,
    pub report: Report,
}

pub mod video;
