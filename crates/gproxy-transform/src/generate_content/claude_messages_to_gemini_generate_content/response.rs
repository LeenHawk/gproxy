use gproxy_protocol::{claude, gemini};

use crate::TransformError;
use crate::models::common::wire_string;

use super::{content, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let mut rest = input.rest;
    crate::common::claude_message_controls::preserve_input_transformations(
        &mut rest,
        input.input_transformations,
    )?;
    let output_tokens = input.usage.output_tokens.map(to_i32).transpose()?;
    let output = gemini::GenerateContentResponse {
        candidates: vec![gemini::Candidate {
            content: Some(content::response_content(input.content)?),
            finish_reason: Some(stop_reason(input.stop_reason)),
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
        }],
        prompt_feedback: None,
        usage_metadata: Some(usage::convert(input.usage)?),
        model_version: Some(wire_string(&input.model)?),
        response_id: Some(input.id),
        model_status: None,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(super) fn stop_reason(reason: claude::StopReason) -> gemini::FinishReason {
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
        claude::StopReason::Unknown(value) => return gemini::FinishReason::Unknown(value),
        _ => gemini::FinishReasonKnown::Other,
    };
    gemini::FinishReason::Known(known)
}

fn to_i32(value: u64) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Claude response", "token count exceeds i32"))
}
