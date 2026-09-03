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
    let mut system_instruction = input.instructions.map(|text| gemini::Content {
        parts: vec![gemini::Part {
            data: Some(gemini::PartData::Text {
                text,
                rest: Default::default(),
            }),
            ..Default::default()
        }],
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
        rest: Default::default(),
    });
    let mut converter = ContentConverter::new();
    let mut contents = Vec::new();
    for content in converter.input(input.input)? {
        if matches!(
            content.role,
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System))
        ) {
            append_system(&mut system_instruction, system_content_text(content)?);
        } else {
            contents.push(content);
        }
    }
    let tools = tools::to_gemini(input.tools)?;
    let tool_config = mixed_tool_config(tools::choice_to_gemini(input.tool_choice), &tools);
    let output = gemini::GenerateContentRequest {
        model: Some(model.to_owned()),
        contents,
        tools,
        tool_config,
        safety_settings: None,
        system_instruction,
        generation_config,
        cached_content: input
            .prompt_cache_key
            .filter(|value| value.starts_with("cachedContents/")),
        service_tier: config::openai_service_tier(input.service_tier),
        store: input.store,
        rest: Default::default(),
    };
    Ok(Bytes::from(serde_json::to_vec(&output)?))
}

fn mixed_tool_config(
    mut config: Option<gemini::ToolConfig>,
    tools: &Option<Vec<gemini::Tool>>,
) -> Option<gemini::ToolConfig> {
    let functions = tools.as_ref().is_some_and(|tools| {
        tools.iter().any(|tool| {
            tool.function_declarations
                .as_ref()
                .is_some_and(|declarations| !declarations.is_empty())
        })
    });
    let built_ins = tools.as_ref().is_some_and(|tools| {
        tools.iter().any(|tool| {
            tool.google_search.is_some()
                || tool.google_search_retrieval.is_some()
                || tool.code_execution.is_some()
                || tool.computer_use.is_some()
                || tool.url_context.is_some()
                || tool.file_search.is_some()
                || tool.google_maps.is_some()
                || tool.mcp_servers.is_some()
        })
    });
    if functions && built_ins {
        config
            .get_or_insert_with(gemini::ToolConfig::default)
            .include_server_side_tool_invocations = Some(true);
    }
    config
}

fn append_system(system: &mut Option<gemini::Content>, text: String) {
    if text.is_empty() {
        return;
    }
    let system = system.get_or_insert_with(|| gemini::Content {
        parts: Vec::new(),
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
        rest: Default::default(),
    });
    system.parts.push(gemini::Part {
        data: Some(gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        ..Default::default()
    });
}

fn system_content_text(content: gemini::Content) -> Result<String, TransformError> {
    let mut text = String::new();
    for part in content.parts {
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) => text.push_str(&value),
            None => {}
            Some(_) => {
                return Err(TransformError::unsupported(
                    "Responses system content",
                    "non-text part",
                ));
            }
        }
    }
    Ok(text)
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
        Some(openai::ResponseFormat::Unknown(_)) => (None, None),
    })
}

fn to_i32(value: u32) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Responses request", "integer exceeds i32"))
}
