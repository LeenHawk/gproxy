//! Operator rewrite rules: compilation at assembly time, selection and
//! application at execution time. Regexes and paths compile once per
//! configuration revision; execution never re-validates rule rows. Visible
//! order is execution order and every rule sees the previous rule's output.

mod apply;
mod compile;
mod json_path;
mod select;
mod stream;

pub use apply::{apply_body, apply_headers, apply_query, apply_unit};
pub use compile::{RewriteCompileError, compile_rule};
pub use select::{Phase, RewriteContext, SelectedRules, select_rules};
pub use stream::StreamRewriter;

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    /// A selected header value is not visible ASCII/UTF-8; it is never decoded lossily.
    #[error("header `{0}` has a value that cannot be processed as text")]
    NonTextHeader(String),
    #[error("rewritten header `{0}` is not a valid header value")]
    InvalidHeaderValue(String),
    #[error("body is not UTF-8 text")]
    NonUtf8Body,
    #[error("stream unit exceeds {0} bytes")]
    UnitTooLarge(u64),
    #[error("malformed JSON array stream")]
    MalformedJsonArray,
}

impl From<RewriteError> for crate::CoreError {
    fn from(error: RewriteError) -> Self {
        Self::Rewrite(error.to_string())
    }
}
