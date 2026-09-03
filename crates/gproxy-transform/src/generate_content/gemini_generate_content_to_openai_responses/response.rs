use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::openai_responses_to_gemini_generate_content::{
    content::ContentConverter, usage,
};

use super::config;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let finish_reason = finish_reason(input.status.as_ref(), input.incomplete_details.as_ref())?;
    let mut converter = ContentConverter::new();
    let contents = input
        .output
        .into_iter()
        .filter_map(|item| converter.item(item).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    let parts = contents
        .into_iter()
        .flat_map(|content| content.parts)
        .collect::<Vec<_>>();
    let mut usage_metadata = usage::to_gemini(input.usage)?;
    if let Some(usage) = usage_metadata.as_mut() {
        usage.service_tier = config::openai_service_tier(input.service_tier);
    }
    let candidate = (!parts.is_empty() || finish_reason.is_some()).then(|| gemini::Candidate {
        content: (!parts.is_empty()).then(|| gemini::Content {
            parts,
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            rest: Default::default(),
        }),
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
        rest: Default::default(),
    });
    let output = gemini::GenerateContentResponse {
        candidates: candidate.into_iter().collect(),
        prompt_feedback: None,
        usage_metadata,
        model_version: input.model.map(config::model_string).transpose()?,
        response_id: Some(input.id),
        model_status: None,
        rest: Default::default(),
    };
    Ok(Bytes::from(serde_json::to_vec(&output)?))
}

pub(in crate::generate_content) fn finish_reason(
    status: Option<&openai::ResponseStatus>,
    details: Option<&openai::IncompleteDetails>,
) -> Result<Option<gemini::FinishReason>, TransformError> {
    Ok(match status {
        Some(openai::ResponseStatus::Completed) => {
            Some(gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop))
        }
        Some(openai::ResponseStatus::Incomplete) => {
            Some(match details.and_then(|details| details.reason.as_ref()) {
                None => gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens),
                Some(openai::IncompleteReason::MaxOutputTokens) => {
                    gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens)
                }
                Some(openai::IncompleteReason::ContentFilter) => {
                    gemini::FinishReason::Known(gemini::FinishReasonKnown::Safety)
                }
                Some(openai::IncompleteReason::Unknown(value)) if value.is_empty() => {
                    gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens)
                }
                Some(openai::IncompleteReason::Unknown(_)) => return Ok(None),
            })
        }
        Some(openai::ResponseStatus::Failed | openai::ResponseStatus::Cancelled) => Some(
            gemini::FinishReason::Known(gemini::FinishReasonKnown::Other),
        ),
        Some(openai::ResponseStatus::Unknown(_)) => None,
        Some(openai::ResponseStatus::InProgress | openai::ResponseStatus::Queued) | None => None,
    })
}
