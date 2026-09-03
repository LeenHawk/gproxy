use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{content, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    let id = input.response_id.unwrap_or_default();
    let model = input.model_version.unwrap_or_default();
    let usage = input
        .usage_metadata
        .map(usage::convert)
        .transpose()?
        .unwrap_or_else(empty_usage);
    let (blocks, stop_reason, stop_sequence) = candidate(input.candidates, input.prompt_feedback)?;
    let output = claude::CreateMessageResponseBody {
        id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: blocks,
        model: model.into(),
        stop_reason,
        stop_sequence,
        usage,
        container: None,
        context_management: None,
        diagnostics: None,
        input_transformations: None,
        stop_details: None,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn candidate(
    candidates: Vec<gemini::Candidate>,
    prompt_feedback: Option<gemini::PromptFeedback>,
) -> Result<
    (
        Vec<claude::ContentBlock>,
        claude::StopReason,
        Option<String>,
    ),
    TransformError,
> {
    let Some(candidate) = candidates.into_iter().next() else {
        if prompt_feedback.map(blocked).transpose()?.unwrap_or(false) {
            return Ok((
                empty_text_response(),
                claude::StopReason::Known(claude::StopReasonKnown::Refusal),
                None,
            ));
        }
        return Ok((
            empty_text_response(),
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
            None,
        ));
    };
    let blocks = candidate
        .content
        .map(content::response_blocks)
        .transpose()?
        .filter(|blocks| !blocks.is_empty())
        .unwrap_or_else(empty_text_response);
    let reason = candidate.finish_reason;
    let has_tool = blocks.iter().any(|block| {
        matches!(
            block,
            claude::ResponseContentBlock::ToolUse(_)
                | claude::ResponseContentBlock::ServerToolUse(_)
        )
    });
    Ok((
        blocks,
        reason.map_or_else(
            || Ok(claude::StopReason::Known(claude::StopReasonKnown::EndTurn)),
            |reason| finish_reason(reason, has_tool),
        )?,
        candidate.finish_message,
    ))
}

fn empty_usage() -> claude::Usage {
    claude::Usage {
        input_tokens: Some(0),
        output_tokens: Some(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        rest: Default::default(),
    }
}

fn empty_text_response() -> Vec<claude::ContentBlock> {
    vec![claude::ContentBlock::Text(claude::ResponseTextBlock {
        citations: None,
        text: String::new(),
        type_: claude::TextBlockType::Text,
        rest: Default::default(),
    })]
}

pub(super) fn finish_reason(
    reason: gemini::FinishReason,
    has_tool: bool,
) -> Result<claude::StopReason, TransformError> {
    Ok(match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop) if has_tool => {
            claude::StopReason::Known(claude::StopReasonKnown::ToolUse)
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop) => {
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => claude::StopReason::Known(claude::StopReasonKnown::Refusal),
        gemini::FinishReason::Unknown(value) => {
            return Err(TransformError::unsupported("Gemini finish reason", value));
        }
        other => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                serde_json::to_string(&other)?,
            ));
        }
    })
}

pub(super) fn blocked(feedback: gemini::PromptFeedback) -> Result<bool, TransformError> {
    match feedback.block_reason {
        None => Ok(false),
        Some(gemini::BlockReason::Known(gemini::BlockReasonKnown::BlockReasonUnspecified)) => Err(
            TransformError::shape("Gemini prompt feedback", "block reason is unspecified"),
        ),
        Some(gemini::BlockReason::Known(_)) => Ok(true),
        Some(gemini::BlockReason::Unknown(value)) => {
            Err(TransformError::unsupported("Gemini block reason", value))
        }
        _ => Err(TransformError::unsupported(
            "Gemini block reason",
            "future variant",
        )),
    }
}
