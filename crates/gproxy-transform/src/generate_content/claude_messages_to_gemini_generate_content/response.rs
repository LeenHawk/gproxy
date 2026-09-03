use gproxy_protocol::{claude, gemini};

use crate::TransformError;
use crate::models::common::wire_string;

use super::{content, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: claude::CreateMessageResponseBody,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    let output_tokens = input.usage.output_tokens.map(to_i32).transpose()?;
    let output = crate::wire!(gemini::GenerateContentResponse {
        candidates: vec![crate::wire!(gemini::Candidate {
            content: Some(content::response_content(input.content)?),
            finish_reason: Some(stop_reason(input.stop_reason)?),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: output_tokens,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: input.stop_sequence,
            rest: Default::default(),
        })],
        prompt_feedback: None,
        usage_metadata: Some(usage::convert(input.usage)?),
        model_version: Some(wire_string(&input.model)?),
        response_id: Some(input.id),
        model_status: None,
        rest: Default::default(),
    });
    Ok(output)
}

pub(super) fn stop_reason(
    reason: claude::StopReason,
) -> Result<gemini::FinishReason, TransformError> {
    let known = match reason {
        claude::StopReason::Known(
            claude::StopReasonKnown::MaxTokens
            | claude::StopReasonKnown::ModelContextWindowExceeded,
        ) => gemini::FinishReasonKnown::MaxTokens,
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
            gemini::FinishReasonKnown::Safety
        }
        claude::StopReason::Known(
            claude::StopReasonKnown::EndTurn
            | claude::StopReasonKnown::StopSequence
            | claude::StopReasonKnown::ToolUse
            | claude::StopReasonKnown::PauseTurn
            | claude::StopReasonKnown::Compaction,
        ) => gemini::FinishReasonKnown::Stop,
        claude::StopReason::Unknown(value) => {
            return Err(TransformError::unsupported("Claude stop reason", value));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Claude stop reason",
                "future reason",
            ));
        }
    };
    Ok(gemini::FinishReason::Known(known))
}

fn to_i32(value: u64) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Claude response", "token count exceeds i32"))
}
