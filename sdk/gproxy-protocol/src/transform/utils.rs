use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformError {
    pub message: Cow<'static, str>,
}

impl TransformError {
    /// Construct a `TransformError` with a static string message.
    ///
    /// Kept for backwards compatibility with `TryFrom` impls that use
    /// compile-time string literals for "not yet supported" cases.
    pub const fn not_implemented(message: &'static str) -> Self {
        Self {
            message: Cow::Borrowed(message),
        }
    }

    /// Construct a `TransformError` with a dynamically-built message.
    ///
    /// Used by the runtime transform dispatcher in `crate::transform::dispatch`
    /// which reports errors like "no stream aggregation for protocol: {protocol}".
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Cow::Owned(message.into()),
        }
    }
}

impl Display for TransformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for TransformError {}

pub type TransformResult<T> = Result<T, TransformError>;

// `push_message_block` lives next to the other Claude-side helpers in
// `transform::claude::utils`. Re-exported here so that callers reach it via
// the generic `transform::utils` path without a cross-module dependency on
// the `claude` submodule.
pub use crate::transform::claude::utils::{ORPHAN_TOOL_USE_PLACEHOLDER_NAME, push_message_block};
