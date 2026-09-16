//! Direct Chat ↔ Claude incremental events, with independent native validation.

mod chat_blocks;
mod chat_to_claude;
mod claude_to_chat;
mod common;
mod usage_facts;

use crate::{
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, KnownIdPrefix, SourceIdentity, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{generate_content as c, stream as s},
        openai::chat::{self as o, stream as q},
    },
};
use std::collections::BTreeMap;

pub use chat_to_claude::{ChatToClaudeContext, ChatToClaudeStream};
pub use claude_to_chat::{ClaudeToChatContext, ClaudeToChatStream};
pub use common::{StreamEnd, StreamLimits};

fn invalid(detail: &'static str) -> TransformError {
    TransformError::invalid_result("claude_chat.stream", detail)
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "claude_chat.stream",
        "stream limit exceeded",
    )
}
