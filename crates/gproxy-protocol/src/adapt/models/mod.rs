//! Bounded full-directory collection and typed cross-vendor return mapping.
//! Collection always begins at the first page. Vendor cursors stay private to
//! the selected upstream; no cursor is renamed into another vendor's token.

mod collect;
mod convert;
mod query;

use crate::{HttpBody, WireResponse, codec::CodecLimits, transform::TransformError};

pub use collect::{collect_claude, collect_gemini, collect_openai};
pub use convert::*;

#[derive(Debug, Clone, Copy)]
pub struct ModelListLimits {
    pub codec: CodecLimits,

    /// Sum of encoded declared page sizes retained by this operation. Limits
    /// both collection memory and the amount of data traversed across pages.
    pub max_declared_bytes: u64,
}

#[derive(Debug)]
pub struct ModelDirectory<T> {
    pub value: T,
    pub completed_calls: usize,
}

#[derive(Debug)]
pub enum ModelListFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}

/// No partial directory is returned as success. `completed_calls` counts only
/// decoded 2xx pages; a failed transport/decode may already have reached the
/// upstream. This counter never authorizes retry.
#[derive(Debug)]
pub struct ModelListError {
    pub completed_calls: usize,
    pub completed_models: usize,
    pub failure: ModelListFailure,
}

impl From<TransformError> for ModelListError {
    fn from(error: TransformError) -> Self {
        Self {
            completed_calls: 0,
            completed_models: 0,
            failure: ModelListFailure::Transform(error),
        }
    }
}
