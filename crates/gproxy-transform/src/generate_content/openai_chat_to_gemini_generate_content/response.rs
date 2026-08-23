use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use crate::generate_content::gemini_generate_content_to_openai_chat::{content, wire};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    let id = input
        .response_id
        .ok_or_else(|| TransformError::shape("Gemini response", "responseId is missing"))?;
    let model = input
        .model_version
        .ok_or_else(|| TransformError::shape("Gemini response", "modelVersion is missing"))?;
    let service_tier = match input
        .usage_metadata
        .as_ref()
        .and_then(|usage| usage.service_tier.clone())
    {
        Some(tier) => wire::service_tier(Some(tier))?,
        None => None,
    };
    let usage = input.usage_metadata.map(wire::usage).transpose()?;
    let mut rest = input.rest;
    preserve(&mut rest, "gemini_prompt_feedback", input.prompt_feedback)?;
    preserve(&mut rest, "gemini_model_status", input.model_status)?;
    let choices = input
        .candidates
        .into_iter()
        .enumerate()
        .map(|(fallback, candidate)| {
            let metadata = candidate_metadata(&candidate);
            let finish_reason = candidate.finish_reason.ok_or_else(|| {
                TransformError::shape("Gemini candidate", "finishReason is missing")
            })?;
            let index = match candidate.index {
                Some(index) => wire::count(index, "candidate.index")?,
                None => u32::try_from(fallback).map_err(|_| {
                    TransformError::shape("Gemini candidate", "fallback index exceeds u32")
                })?,
            };
            let mut choice_rest = candidate.rest;
            preserve(&mut choice_rest, "gemini_candidate_metadata", metadata)?;
            Ok(openai::ChatCompletionChoice {
                finish_reason: wire::finish_reason(finish_reason)?,
                index,
                logprobs: None,
                message: content::message(candidate.content, fallback)?,
                rest: choice_rest,
            })
        })
        .collect::<Result<Vec<_>, TransformError>>()?;
    let output = openai::ChatCompletionResponse {
        id,
        choices,
        created: None,
        model: model.into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier,
        system_fingerprint: None,
        usage,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn candidate_metadata(candidate: &gemini::Candidate) -> Option<serde_json::Value> {
    let value = serde_json::json!({
        "safetyRatings": candidate.safety_ratings,
        "citationMetadata": candidate.citation_metadata,
        "tokenCount": candidate.token_count,
        "groundingMetadata": candidate.grounding_metadata,
        "avgLogprobs": candidate.avg_logprobs,
        "logprobsResult": candidate.logprobs_result,
        "urlContextMetadata": candidate.url_context_metadata,
        "finishMessage": candidate.finish_message,
    });
    value
        .as_object()
        .is_some_and(|map| map.values().any(|value| !value.is_null()))
        .then_some(value)
}

fn preserve<T: serde::Serialize>(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        rest.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}
