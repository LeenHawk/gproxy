use gproxy_protocol::{gemini, openai};

use crate::TransformError;

mod format;

pub(crate) struct ChatConfig {
    pub frequency_penalty: Option<f64>,
    pub logprobs: Option<bool>,
    pub max_tokens: Option<u32>,
    pub modalities: Option<Vec<openai::TextOrAudioModality>>,
    pub n: Option<u32>,
    pub presence_penalty: Option<f64>,
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    pub response_format: Option<openai::ChatResponseFormat>,
    pub seed: Option<i64>,
    pub stop: Option<openai::StringOrList>,
    pub temperature: Option<f64>,
    pub top_logprobs: Option<u32>,
    pub top_p: Option<f64>,
}

pub(crate) fn to_chat(
    config: Option<gemini::GenerationConfig>,
) -> Result<ChatConfig, TransformError> {
    let Some(config) = config else {
        return Ok(empty());
    };
    let audio_requested = config.response_modalities.as_ref().is_some_and(|values| {
        values.iter().any(|value| {
            matches!(
                value,
                gemini::ResponseModality::Known(gemini::ResponseModalityKnown::Audio)
            )
        })
    });
    if audio_requested
        || config.speech_config.is_some()
        || config
            .response_format
            .as_ref()
            .is_some_and(|format| format.audio.is_some())
    {
        return Err(TransformError::unsupported(
            "Gemini request",
            "audio response conversion requires unavailable request context",
        ));
    }
    let response_format = format::convert(&config)?;
    let modalities = config.response_modalities.map(modalities).transpose()?;
    Ok(ChatConfig {
        frequency_penalty: config.frequency_penalty,
        logprobs: config.response_logprobs,
        max_tokens: config
            .max_output_tokens
            .map(|value| unsigned(value, "maxOutputTokens"))
            .transpose()?,
        modalities,
        n: config
            .candidate_count
            .map(|value| unsigned(value, "candidateCount"))
            .transpose()?,
        presence_penalty: config.presence_penalty,
        reasoning_effort: reasoning(config.thinking_config.as_ref())?,
        response_format,
        seed: config.seed,
        stop: config.stop_sequences.map(openai::StringOrList::List),
        temperature: config.temperature,
        top_logprobs: config
            .logprobs
            .map(|value| unsigned(value, "logprobs"))
            .transpose()?,
        top_p: config.top_p,
    })
}

fn empty() -> ChatConfig {
    ChatConfig {
        frequency_penalty: None,
        logprobs: None,
        max_tokens: None,
        modalities: None,
        n: None,
        presence_penalty: None,
        reasoning_effort: None,
        response_format: None,
        seed: None,
        stop: None,
        temperature: None,
        top_logprobs: None,
        top_p: None,
    }
}

fn modalities(
    modalities: Vec<gemini::ResponseModality>,
) -> Result<Vec<openai::TextOrAudioModality>, TransformError> {
    if modalities.is_empty() {
        return Ok(vec![openai::TextOrAudioModality::Text]);
    }
    Ok(modalities
        .into_iter()
        .filter_map(|modality| match modality {
            gemini::ResponseModality::Known(gemini::ResponseModalityKnown::Text) => {
                Some(openai::TextOrAudioModality::Text)
            }
            gemini::ResponseModality::Known(gemini::ResponseModalityKnown::Audio) => {
                Some(openai::TextOrAudioModality::Audio)
            }
            _ => None,
        })
        .collect())
}

fn reasoning(
    config: Option<&gemini::ThinkingConfig>,
) -> Result<Option<openai::ReasoningEffort>, TransformError> {
    let Some(config) = config else {
        return Ok(None);
    };
    if config.include_thoughts == Some(false) {
        return Ok(Some(openai::ReasoningEffort::None));
    }
    let Some(level) = config.thinking_level.as_ref() else {
        return Ok(None);
    };
    Ok(Some(match level {
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal) => {
            openai::ReasoningEffort::Minimal
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low) => {
            openai::ReasoningEffort::Low
        }
        gemini::ThinkingLevel::Known(
            gemini::ThinkingLevelKnown::Medium
            | gemini::ThinkingLevelKnown::ThinkingLevelUnspecified,
        ) => openai::ReasoningEffort::Medium,
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High) => {
            openai::ReasoningEffort::High
        }
        _ => return Ok(None),
    }))
}

fn unsigned(value: i32, field: &'static str) -> Result<u32, TransformError> {
    u32::try_from(value).map_err(|_| {
        TransformError::shape("Gemini generation config", format!("{field} is negative"))
    })
}
