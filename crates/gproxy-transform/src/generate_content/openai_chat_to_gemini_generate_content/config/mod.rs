use gproxy_protocol::{gemini, openai};

use crate::TransformError;

mod format;

pub(crate) struct Input {
    pub audio: Option<openai::ChatAudioParam>,
    pub frequency_penalty: Option<f64>,
    pub logprobs: Option<bool>,
    pub max_tokens: Option<u32>,
    pub modalities: Option<Vec<openai::TextOrAudioModality>>,
    pub n: Option<u32>,
    pub presence_penalty: Option<f64>,
    pub reasoning: Option<openai::ReasoningEffort>,
    pub response_format: Option<openai::ChatResponseFormat>,
    pub seed: Option<i64>,
    pub stop: Option<openai::StringOrList>,
    pub temperature: Option<f64>,
    pub top_logprobs: Option<u32>,
    pub top_p: Option<f64>,
}

pub(crate) fn to_gemini(input: Input) -> Result<Option<gemini::GenerationConfig>, TransformError> {
    if input.audio.is_some()
        || input.modalities.as_ref().is_some_and(|values| {
            values
                .iter()
                .any(|value| matches!(value, openai::TextOrAudioModality::Audio))
        })
    {
        return Err(TransformError::unsupported(
            "Chat request",
            "audio response conversion requires unavailable request context",
        ));
    }
    let response_modalities = input.modalities.map(modalities).transpose()?;
    let (response_mime_type, response_json_schema) = format::convert(input.response_format)?;
    let config = gemini::GenerationConfig {
        stop_sequences: input.stop.map(stop),
        response_mime_type,
        response_schema: None,
        private_response_json_schema: None,
        response_json_schema,
        response_format: None,
        response_modalities,
        candidate_count: input.n.map(|value| signed(value, "n")).transpose()?,
        max_output_tokens: input
            .max_tokens
            .map(|value| signed(value, "max_completion_tokens"))
            .transpose()?,
        temperature: input.temperature,
        top_p: input.top_p,
        top_k: None,
        seed: input.seed,
        presence_penalty: input.presence_penalty,
        frequency_penalty: input.frequency_penalty,
        response_logprobs: input.logprobs,
        logprobs: input
            .top_logprobs
            .map(|value| signed(value, "top_logprobs"))
            .transpose()?,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config: reasoning(input.reasoning),
        image_config: None,
        media_resolution: None,
        rest: Default::default(),
    };
    Ok(has_values(&config).then_some(config))
}

fn modalities(
    values: Vec<openai::TextOrAudioModality>,
) -> Result<Vec<gemini::ResponseModality>, TransformError> {
    values
        .into_iter()
        .filter_map(|value| match value {
            openai::TextOrAudioModality::Text => Some(Ok(gemini::ResponseModality::Known(
                gemini::ResponseModalityKnown::Text,
            ))),
            openai::TextOrAudioModality::Audio => Some(Ok(gemini::ResponseModality::Known(
                gemini::ResponseModalityKnown::Audio,
            ))),
            openai::TextOrAudioModality::Unknown(_) => None,
        })
        .collect()
}

fn stop(stop: openai::StringOrList) -> Vec<String> {
    match stop {
        openai::StringOrList::String(value) => vec![value],
        openai::StringOrList::List(values) => values,
    }
}

fn reasoning(effort: Option<openai::ReasoningEffort>) -> Option<gemini::ThinkingConfig> {
    let effort = effort?;
    let (include_thoughts, level) = match effort {
        openai::ReasoningEffort::None => (Some(false), None),
        openai::ReasoningEffort::Minimal => (
            Some(true),
            Some(gemini::ThinkingLevel::Known(
                gemini::ThinkingLevelKnown::Minimal,
            )),
        ),
        openai::ReasoningEffort::Low => (
            Some(true),
            Some(gemini::ThinkingLevel::Known(
                gemini::ThinkingLevelKnown::Low,
            )),
        ),
        openai::ReasoningEffort::Medium => (
            Some(true),
            Some(gemini::ThinkingLevel::Known(
                gemini::ThinkingLevelKnown::Medium,
            )),
        ),
        openai::ReasoningEffort::High
        | openai::ReasoningEffort::XHigh
        | openai::ReasoningEffort::Max => (
            Some(true),
            Some(gemini::ThinkingLevel::Known(
                gemini::ThinkingLevelKnown::High,
            )),
        ),
        openai::ReasoningEffort::Unknown(_) => return None,
    };
    Some(gemini::ThinkingConfig {
        include_thoughts,
        thinking_budget: None,
        thinking_level: level,
        rest: Default::default(),
    })
}

fn signed(value: u32, field: &'static str) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Chat request", format!("{field} exceeds i32")))
}

fn has_values(config: &gemini::GenerationConfig) -> bool {
    serde_json::to_value(config)
        .is_ok_and(|value| value.as_object().is_some_and(|value| !value.is_empty()))
}
