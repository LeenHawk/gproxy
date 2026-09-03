use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::models::common::wire_string;

use crate::generate_content::openai_chat_to_gemini_generate_content::{content, wire};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ChatCompletionResponse,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    let service_tier = input
        .service_tier
        .and_then(|tier| wire::service_tier(Some(tier)));
    let mut usage_metadata = input.usage.map(v2_usage);
    if let Some(usage) = usage_metadata.as_mut() {
        usage.service_tier = service_tier;
    }
    let candidates = input
        .choices
        .into_iter()
        .map(|choice| {
            Ok(crate::wire!(gemini::Candidate {
                content: Some(content::candidate(choice.message)?),
                finish_reason: Some(wire::finish_reason(choice.finish_reason)?),
                safety_ratings: Vec::new(),
                citation_metadata: None,
                token_count: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(wire::index(choice.index)?),
                finish_message: None,
                rest: Default::default(),
            }))
        })
        .collect::<Result<Vec<_>, TransformError>>()?;
    let output = crate::wire!(gemini::GenerateContentResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata,
        model_version: Some(wire_string(&input.model)?),
        response_id: Some(input.id),
        model_status: None,
        rest: Default::default(),
    });
    Ok(output)
}

fn v2_usage(usage: openai::CompletionUsage) -> gemini::UsageMetadata {
    crate::wire!(gemini::UsageMetadata {
        prompt_token_count: Some(clamp(usage.prompt_tokens)),
        cached_content_token_count: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(clamp),
        candidates_token_count: Some(clamp(usage.completion_tokens)),
        thoughts_token_count: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .map(clamp),
        total_token_count: Some(clamp(usage.total_tokens)),
        tool_use_prompt_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        rest: Default::default(),
    })
}

fn clamp(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}
