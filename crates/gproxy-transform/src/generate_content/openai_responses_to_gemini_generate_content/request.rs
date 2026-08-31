use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::content::ContentConverter;
use super::tools;
use crate::generate_content::gemini_generate_content_to_openai_responses::config;

pub(crate) fn transform(body: Bytes, model: &str, _stream: bool) -> Result<Bytes, TransformError> {
    let input: openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    let effort = input
        .reasoning
        .as_ref()
        .and_then(|value| value.effort.clone());
    let (response_mime_type, response_json_schema) = response_format(input.text)?;
    let thinking_config = config::openai_reasoning(effort);
    let max_output_tokens = input.max_output_tokens.map(to_i32).transpose()?;
    let logprobs = input.top_logprobs.map(to_i32).transpose()?;
    let generation = gemini::GenerationConfig {
        stop_sequences: None,
        response_mime_type,
        response_schema: None,
        private_response_json_schema: None,
        response_json_schema,
        response_format: None,
        response_modalities: None,
        candidate_count: None,
        max_output_tokens,
        temperature: input.temperature,
        top_p: input.top_p,
        top_k: None,
        seed: None,
        presence_penalty: None,
        frequency_penalty: None,
        response_logprobs: None,
        logprobs,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config,
        image_config: None,
        media_resolution: None,
        rest: Default::default(),
    };
    let generation_config = has_generation_fields(&generation).then_some(generation);
    let system_instruction = input.instructions.map(|text| gemini::Content {
        parts: vec![gemini::Part {
            data: Some(gemini::PartData::Text {
                text,
                rest: Default::default(),
            }),
            ..Default::default()
        }],
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
        rest: Default::default(),
    });
    let mut converter = ContentConverter::new();
    let output = gemini::GenerateContentRequest {
        model: Some(model.to_owned()),
        contents: converter.input(input.input)?,
        tools: tools::to_gemini(input.tools)?,
        tool_config: tools::choice_to_gemini(input.tool_choice),
        safety_settings: None,
        system_instruction,
        generation_config,
        cached_content: input.prompt_cache_key,
        service_tier: config::openai_service_tier(input.service_tier),
        store: input.store,
        rest: input.rest,
    };
    Ok(Bytes::from(serde_json::to_vec(&output)?))
}

fn has_generation_fields(config: &gemini::GenerationConfig) -> bool {
    config.response_mime_type.is_some()
        || config.response_json_schema.is_some()
        || config.max_output_tokens.is_some()
        || config.temperature.is_some()
        || config.top_p.is_some()
        || config.logprobs.is_some()
        || config.thinking_config.is_some()
}

fn response_format(
    text: Option<openai::TextConfig>,
) -> Result<(Option<gemini::ResponseMimeType>, Option<serde_json::Value>), TransformError> {
    Ok(match text.and_then(|value| value.format) {
        None => (None, None),
        Some(openai::ResponseFormat::Text(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::TextPlain,
            )),
            None,
        ),
        Some(openai::ResponseFormat::JsonObject(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            None,
        ),
        Some(openai::ResponseFormat::JsonSchema(format)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            Some(serde_json::Value::Object(format.schema)),
        ),
        Some(openai::ResponseFormat::Unknown(raw)) => {
            return Err(TransformError::unsupported(
                "Responses text format",
                raw.to_string(),
            ));
        }
    })
}

fn to_i32(value: u32) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Responses request", "integer exceeds i32"))
}
