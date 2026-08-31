use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use crate::generate_content::gemini_generate_content_to_openai_chat::{content, wire};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    let id = input.response_id.unwrap_or_default();
    let model = input.model_version.unwrap_or_else(|| "unknown".into());
    let service_tier = match input
        .usage_metadata
        .as_ref()
        .and_then(|usage| usage.service_tier.clone())
    {
        Some(tier) => wire::service_tier(Some(tier))?,
        None => None,
    };
    let usage = input.usage_metadata.map(v2_usage);
    let mut rest = input.rest;
    preserve(&mut rest, "gemini_prompt_feedback", input.prompt_feedback)?;
    preserve(&mut rest, "gemini_model_status", input.model_status)?;
    let choices = input
        .candidates
        .into_iter()
        .enumerate()
        .map(|(fallback, candidate)| {
            let metadata = candidate_metadata(&candidate);
            let finish_reason = candidate.finish_reason;
            let index = match candidate.index {
                Some(index) => u32::try_from(index).unwrap_or_default(),
                None => u32::try_from(fallback).map_err(|_| {
                    TransformError::shape("Gemini candidate", "fallback index exceeds u32")
                })?,
            };
            let mut choice_rest = candidate.rest;
            preserve(&mut choice_rest, "gemini_candidate_metadata", metadata)?;
            Ok(openai::ChatCompletionChoice {
                finish_reason: finish_reason
                    .map(wire::finish_reason)
                    .transpose()?
                    .unwrap_or(openai::ChatFinishReason::Stop),
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
        created: Some(0),
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

fn v2_usage(usage: gemini::UsageMetadata) -> openai::CompletionUsage {
    let prompt_tokens = usage
        .prompt_token_count
        .map(nonnegative)
        .unwrap_or_default();
    let completion_tokens = usage
        .candidates_token_count
        .map(nonnegative)
        .unwrap_or_default();
    openai::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens: usage
            .total_token_count
            .map(nonnegative)
            .unwrap_or_else(|| prompt_tokens.saturating_add(completion_tokens)),
        completion_tokens_details: usage.thoughts_token_count.map(|tokens| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(nonnegative(tokens)),
                rejected_prediction_tokens: None,
                rest: Default::default(),
            }
        }),
        prompt_tokens_details: usage.cached_content_token_count.map(|tokens| {
            openai::PromptTokensDetails {
                audio_tokens: None,
                cache_write_tokens: None,
                cached_tokens: Some(nonnegative(tokens)),
                rest: Default::default(),
            }
        }),
        rest: Default::default(),
    }
}

fn nonnegative(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
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
