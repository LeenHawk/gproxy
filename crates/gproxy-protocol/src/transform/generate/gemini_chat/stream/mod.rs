//! Direct incremental Chat ↔ Gemini streams, with native source validation.
mod chat_to_gemini;
mod common;
mod gemini_to_chat;
mod tools;
use crate::{
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{
        DeclaredFields, gemini as g,
        openai::chat::{self as c, stream as s},
    },
};
pub use chat_to_gemini::ChatToGeminiStream;
pub use common::{StreamEnd, StreamLimits};
pub use gemini_to_chat::{GeminiToChatContext, GeminiToChatStream};
use std::collections::BTreeMap;
fn invalid(message: &'static str) -> TransformError {
    TransformError::invalid_result("chat_gemini.stream", message)
}
fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "chat_gemini.stream",
        "stream limit exceeded",
    )
}
