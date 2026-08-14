use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::response_item_to_gemini_content;
use super::usage::response_usage_to_gemini;

pub fn response(
    input: openai::ResponseObject,
    _: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    let mut parts = Vec::new();
    for item in input.output {
        if let Some(content) = response_item_to_gemini_content(item.0) {
            parts.extend(content.parts);
        }
    }
    let finish_reason = match input.status {
        Some(openai::ResponseStatus::Incomplete) => Some(gemini::FinishReason::Known(
            gemini::FinishReasonKnown::MaxTokens,
        )),
        _ => Some(gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop)),
    };
    Ok(crate::protocol::wire!(gemini::GenerateContentResponse {
        candidates: vec![crate::protocol::wire!(gemini::Candidate {
            content: Some(crate::protocol::wire!(gemini::Content {
                parts,
                role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
                extra: Default::default(),
            })),
            finish_reason,
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
            extra: Default::default(),
        })],
        prompt_feedback: None,
        usage_metadata: response_usage_to_gemini(input.usage),
        model_version: input.model.map(common::openai_model_string),
        response_id: Some(input.id),
        extra: Default::default(),
    }))
}
