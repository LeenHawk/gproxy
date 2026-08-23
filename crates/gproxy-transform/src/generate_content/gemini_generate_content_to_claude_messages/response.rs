use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{content, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    let id = input
        .response_id
        .ok_or_else(|| TransformError::shape("Gemini response", "responseId is missing"))?;
    let model = input
        .model_version
        .ok_or_else(|| TransformError::shape("Gemini response", "modelVersion is missing"))?;
    let usage = input
        .usage_metadata
        .map(usage::convert)
        .transpose()?
        .ok_or_else(|| TransformError::shape("Gemini response", "usageMetadata is missing"))?;
    let mut rest = input.rest;
    preserve(&mut rest, "promptFeedback", input.prompt_feedback.as_ref())?;
    preserve(&mut rest, "modelStatus", input.model_status.as_ref())?;
    let (blocks, stop_reason, stop_sequence) =
        candidate(input.candidates, input.prompt_feedback, &mut rest)?;
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
        stop_details: None,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn candidate(
    candidates: Vec<gemini::Candidate>,
    prompt_feedback: Option<gemini::PromptFeedback>,
    rest: &mut claude::JsonObject,
) -> Result<
    (
        Vec<claude::ContentBlock>,
        claude::StopReason,
        Option<String>,
    ),
    TransformError,
> {
    if candidates.len() > 1 {
        return Err(TransformError::unsupported(
            "Gemini response",
            "multiple candidates",
        ));
    }
    let Some(candidate) = candidates.into_iter().next() else {
        if prompt_feedback.map(blocked).transpose()?.unwrap_or(false) {
            return Ok((
                Vec::new(),
                claude::StopReason::Known(claude::StopReasonKnown::Refusal),
                None,
            ));
        }
        return Err(TransformError::shape(
            "Gemini response",
            "candidate is missing",
        ));
    };
    preserve(rest, "safetyRatings", Some(&candidate.safety_ratings))?;
    preserve(
        rest,
        "citationMetadata",
        candidate.citation_metadata.as_ref(),
    )?;
    preserve(rest, "tokenCount", candidate.token_count.as_ref())?;
    preserve(
        rest,
        "groundingMetadata",
        candidate.grounding_metadata.as_ref(),
    )?;
    preserve(rest, "avgLogprobs", candidate.avg_logprobs.as_ref())?;
    preserve(rest, "logprobsResult", candidate.logprobs_result.as_ref())?;
    preserve(
        rest,
        "urlContextMetadata",
        candidate.url_context_metadata.as_ref(),
    )?;
    preserve(rest, "candidateIndex", candidate.index.as_ref())?;
    rest.extend(candidate.rest);
    let blocks = candidate
        .content
        .map(content::response_blocks)
        .transpose()?
        .unwrap_or_else(Vec::new);
    let reason = candidate
        .finish_reason
        .ok_or_else(|| TransformError::shape("Gemini response", "finishReason is missing"))?;
    let has_tool = blocks.iter().any(|block| {
        matches!(
            block,
            claude::ResponseContentBlock::ToolUse(_)
                | claude::ResponseContentBlock::ServerToolUse(_)
        )
    });
    Ok((
        blocks,
        finish_reason(reason, has_tool)?,
        candidate.finish_message,
    ))
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
        gemini::FinishReason::Unknown(value) => claude::StopReason::Unknown(value),
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

fn preserve<T: serde::Serialize>(
    rest: &mut claude::JsonObject,
    key: &str,
    value: Option<&T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        let value = serde_json::to_value(value)?;
        if !value.is_null() && value != serde_json::Value::Array(Vec::new()) {
            rest.insert(key.into(), value);
        }
    }
    Ok(())
}
