use super::super::{
    GenerationResources, GenerationStateAccess, chat_claude::ChatViaClaude,
    chat_responses::ChatViaResponses, claude_gemini::GeminiViaClaude,
    gemini_responses::GeminiViaResponses,
};
use super::*;
use crate::{
    capability::{ResourceAccess, StateStore, Upstream},
    transform::Converted,
    wire::{
        DeclaredFields,
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};

#[derive(Debug)]
pub struct FanoutTarget {
    pub endpoint: Endpoint,
    pub options: FanoutOptions,
}

mod chat_claude;
pub use chat_claude::ChatViaClaudeFanout;

mod chat_responses;
pub use chat_responses::ChatViaResponsesFanout;

mod gemini_claude;
pub use gemini_claude::GeminiViaClaudeFanout;

mod gemini_responses;
pub use gemini_responses::GeminiViaResponsesFanout;
