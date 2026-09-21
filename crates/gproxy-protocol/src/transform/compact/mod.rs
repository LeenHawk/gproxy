//! Codex client remote compaction through ordinary typed generation requests.
//! Returns the native client output array, not the distinct public compaction
//! resource envelope and not invented encrypted compaction content.

mod media;
mod request;
mod response;
pub use request::{
    COMPACTION_INSTRUCTION, CompactDialectRequest, CompactDialectRequestBody,
    CompactionRequestContext, PreparedCompactRequest, build_request,
};
pub use response::replacement_history;
