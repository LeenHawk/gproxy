use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::models::common::wire_string;

use crate::generate_content::openai_chat_to_gemini_generate_content::{content, wire};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let mut rest = input.rest;
    if let Some(created) = input.created {
        rest.insert("openai_created".into(), created.into());
    }
    if let Some(fingerprint) = input.system_fingerprint {
        rest.insert("openai_system_fingerprint".into(), fingerprint.into());
    }
    if let Some(moderation) = input.moderation {
        rest.insert(
            "openai_moderation".into(),
            serde_json::to_value(moderation)?,
        );
    }
    let service_tier = input
        .service_tier
        .and_then(|tier| wire::service_tier(Some(tier)));
    let mut usage_metadata = input.usage.map(wire::usage).transpose()?;
    if let Some(usage) = usage_metadata.as_mut() {
        usage.service_tier = service_tier;
    } else if let Some(tier) = service_tier {
        rest.insert("gemini_service_tier".into(), serde_json::to_value(tier)?);
    }
    let candidates = input
        .choices
        .into_iter()
        .map(|choice| {
            Ok(gemini::Candidate {
                content: Some(content::candidate(choice.message)?),
                finish_reason: Some(wire::finish_reason(choice.finish_reason)),
                safety_ratings: Vec::new(),
                citation_metadata: None,
                token_count: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(wire::index(choice.index)?),
                finish_message: None,
                rest: choice.rest,
            })
        })
        .collect::<Result<Vec<_>, TransformError>>()?;
    let output = gemini::GenerateContentResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata,
        model_version: Some(wire_string(&input.model)?),
        response_id: Some(input.id),
        model_status: None,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
