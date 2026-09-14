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
    pub model: String,
    pub endpoint: Endpoint,
    pub identities: Vec<GenerationIdentity>,
    pub options: FanoutOptions,
}
fn validate(target: &FanoutTarget, count: i64) -> Result<String, TransformError> {
    target.endpoint.validate()?;
    if usize::try_from(count).ok() != Some(target.identities.len()) {
        return Err(TransformError::shape(
            "fanout.count",
            "candidate count must match distinct prepared child identities",
        ));
    }
    group_id(target.options, &target.identities)
}
fn chat_count(input: &h::GenerateContentRequestBody) -> Result<i64, TransformError> {
    if input.stream.flatten() == Some(true) {
        return Err(TransformError::unsupported(
            "stream",
            "use incremental fanout invocation",
        ));
    }
    Ok(input.n.flatten().unwrap_or(1))
}
fn gemini_count(input: &g::GenerateContentRequestBody) -> Result<i64, TransformError> {
    Ok(input
        .generation_config
        .as_ref()
        .and_then(|c| c.candidate_count)
        .unwrap_or(1))
}

mod chat_claude;
pub use chat_claude::ChatViaClaudeFanout;

mod chat_responses;
pub use chat_responses::ChatViaResponsesFanout;

mod gemini_claude;
pub use gemini_claude::GeminiViaClaudeFanout;

mod gemini_responses;
pub use gemini_responses::GeminiViaResponsesFanout;
