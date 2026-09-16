//! Direct vendor-wire conversions and the identity rules they share.
//!
//! A conversion only reads declared source fields and builds declared target
//! fields. Unknown `rest` members are neither conversion input nor output.
//! Conversion state and diagnostics are separate from vendor wire payloads.

pub mod compact;
pub mod count_tokens;
pub mod embeddings;
mod error;
pub mod files;
pub mod generate;
pub mod guardian;
pub mod identity;
pub mod images;
mod instructions;
pub mod memory;
pub mod models;

pub use error::{Diagnostic, DiagnosticKind, Report, TransformError, TransformErrorKind};

/// An unrepresentable field or item is omitted at its mapping boundary.
/// Parsing, host and state errors still propagate to the caller.
pub(crate) fn optional<T>(value: Result<T, TransformError>) -> Result<Option<T>, TransformError> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(error)
            if matches!(
                error.kind(),
                TransformErrorKind::Unsupported | TransformErrorKind::MissingMetadata
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// A typed target value and out-of-band field diagnostics. This wrapper never
/// changes or normalizes the payload type into a common content model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted<T> {
    pub value: T,
    pub report: Report,
}

pub mod video;
