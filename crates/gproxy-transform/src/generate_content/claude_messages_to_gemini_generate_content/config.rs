use gproxy_protocol::{claude, gemini};

use crate::TransformError;

#[allow(deprecated)] // Reading the legacy Claude output_format remains necessary on the wire.
pub(super) fn generation(
    input: &claude::CreateMessageRequestBody,
) -> Result<gemini::GenerationConfig, TransformError> {
    let format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or_else(|| input.output_format.clone());
    let effort = input
        .output_config
        .as_ref()
        .and_then(|config| config.effort.clone());
    let mut thinking_config = thinking_to_gemini(input.thinking.clone())?;
    if let Some(level) = effort_to_gemini(effort) {
        thinking_config
            .get_or_insert_with(|| gemini::ThinkingConfig {
                include_thoughts: None,
                thinking_budget: None,
                thinking_level: None,
                rest: Default::default(),
            })
            .thinking_level = Some(level);
    }
    let (response_mime_type, response_json_schema) = match format {
        Some(format) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            Some(serde_json::to_value(format.schema)?),
        ),
        None => (None, None),
    };
    Ok(gemini::GenerationConfig {
        stop_sequences: input.stop_sequences.clone(),
        response_mime_type,
        response_schema: None,
        private_response_json_schema: None,
        response_json_schema,
        response_format: None,
        response_modalities: None,
        candidate_count: None,
        max_output_tokens: Some(to_i32(input.max_tokens)?),
        temperature: input.temperature,
        top_p: input.top_p,
        top_k: input.top_k.map(signed_to_i32).transpose()?,
        seed: None,
        presence_penalty: None,
        frequency_penalty: None,
        response_logprobs: None,
        logprobs: None,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config,
        image_config: None,
        media_resolution: None,
        rest: Default::default(),
    })
}

pub(super) fn request_tier(
    speed: Option<claude::Speed>,
    tier: Option<claude::RequestServiceTier>,
) -> Option<gemini::ServiceTier> {
    if matches!(speed, Some(claude::Speed::Known(claude::SpeedKnown::Fast))) {
        return Some(gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Priority,
        ));
    }
    tier.and_then(|tier| match tier {
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto) => Some(
            gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified),
        ),
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::StandardOnly) => Some(
            gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard),
        ),
        claude::RequestServiceTier::Unknown(_) => None,
        _ => None,
    })
}

fn thinking_to_gemini(
    thinking: Option<claude::ThinkingConfig>,
) -> Result<Option<gemini::ThinkingConfig>, TransformError> {
    let Some(thinking) = thinking else {
        return Ok(None);
    };
    Ok(Some(match thinking {
        claude::ThinkingConfig::Disabled(_) => gemini::ThinkingConfig {
            include_thoughts: Some(false),
            thinking_budget: None,
            thinking_level: None,
            rest: Default::default(),
        },
        claude::ThinkingConfig::Enabled(config) => gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: Some(to_i32(config.budget_tokens)?),
            thinking_level: None,
            rest: Default::default(),
        },
        claude::ThinkingConfig::Adaptive(_) => gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: None,
            thinking_level: None,
            rest: Default::default(),
        },
        claude::ThinkingConfig::Unknown(_) => return Ok(None),
        _ => return Ok(None),
    }))
}

fn effort_to_gemini(effort: Option<claude::OutputEffort>) -> Option<gemini::ThinkingLevel> {
    Some(match effort? {
        claude::OutputEffort::Known(claude::OutputEffortKnown::Low) => {
            gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low)
        }
        claude::OutputEffort::Known(claude::OutputEffortKnown::Medium) => {
            gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Medium)
        }
        claude::OutputEffort::Known(
            claude::OutputEffortKnown::High
            | claude::OutputEffortKnown::XHigh
            | claude::OutputEffortKnown::Max,
        ) => gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High),
        claude::OutputEffort::Unknown(_) => return None,
        _ => return None,
    })
}

fn to_i32(value: u64) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Claude request", "token count exceeds i32"))
}

fn signed_to_i32(value: i64) -> Result<i32, TransformError> {
    i32::try_from(value).map_err(|_| TransformError::shape("Claude request", "topK exceeds i32"))
}
