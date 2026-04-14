use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformError {
    pub message: &'static str,
}

impl TransformError {
    pub const fn not_implemented(message: &'static str) -> Self {
        Self { message }
    }
}

impl Display for TransformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}

impl Error for TransformError {}

pub type TransformResult<T> = Result<T, TransformError>;

// `push_message_block` lives next to the other Claude-side helpers in
// `transform::claude::utils`. Re-exported here so that callers reach it via
// the generic `transform::utils` path without a cross-module dependency on
// the `claude` submodule.
pub use crate::transform::claude::utils::{ORPHAN_TOOL_USE_PLACEHOLDER_NAME, push_message_block};
