use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{config, content::ContentConverter, tools};

pub(crate) fn transform(body: Bytes, model: &str, stream: bool) -> Result<Bytes, TransformError> {
    let input: gemini::GenerateContentRequest = serde_json::from_slice(&body)?;
    let generation = input.generation_config;
    let mut converter = ContentConverter::new();
    let output = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(
            converter.request(input.contents)?,
        )),
        instructions: input.system_instruction.map(system_text).transpose()?,
        max_output_tokens: generation
            .as_ref()
            .and_then(|value| value.max_output_tokens)
            .map(nonnegative)
            .transpose()?,
        max_tool_calls: None,
        metadata: None,
        model: Some(model.into()),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        prompt_cache_key: input.cached_content,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        prompt: None,
        reasoning: config::gemini_reasoning(
            generation
                .as_ref()
                .and_then(|value| value.thinking_config.as_ref()),
        )
        .map(|effort| openai::ReasoningConfig {
            context: None,
            effort: Some(effort),
            mode: None,
            summary: None,
            generate_summary: None,
            rest: Default::default(),
        }),
        safety_identifier: None,
        service_tier: config::gemini_service_tier(input.service_tier),
        store: input.store,
        stream: Some(stream),
        stream_options: None,
        temperature: generation.as_ref().and_then(|value| value.temperature),
        text: generation.as_ref().map(text_config).transpose()?.flatten(),
        tool_choice: tools::choice_to_responses(input.tool_config)?,
        tools: tools::to_responses(input.tools)?,
        top_logprobs: generation
            .as_ref()
            .and_then(|value| value.logprobs)
            .map(nonnegative)
            .transpose()?,
        top_p: generation.as_ref().and_then(|value| value.top_p),
        truncation: None,
        user: None,
        rest: input.rest,
    };
    Ok(Bytes::from(serde_json::to_vec(&output)?))
}

fn system_text(content: gemini::Content) -> Result<String, TransformError> {
    if !content.rest.is_empty()
        || content.role.as_ref().is_some_and(|role| {
            !matches!(
                role,
                gemini::ContentRole::Known(gemini::ContentRoleKnown::System)
            )
        })
    {
        return Err(TransformError::unsupported(
            "Gemini systemInstruction",
            "role or extension fields",
        ));
    }
    let mut text = String::new();
    for part in content.parts {
        if part.thought.is_some()
            || part.thought_signature.is_some()
            || part.part_metadata.is_some()
            || part.media_resolution.is_some()
            || part.metadata.is_some()
            || !part.rest.is_empty()
        {
            return Err(TransformError::unsupported(
                "Gemini systemInstruction",
                "part metadata or extension fields",
            ));
        }
        match part.data {
            Some(gemini::PartData::Text { text: value, rest }) if rest.is_empty() => {
                text.push_str(&value)
            }
            None => {}
            Some(_) => {
                return Err(TransformError::unsupported(
                    "Gemini systemInstruction",
                    "non-text content",
                ));
            }
        }
    }
    Ok(text)
}

fn text_config(
    config: &gemini::GenerationConfig,
) -> Result<Option<openai::TextConfig>, TransformError> {
    let schema = config
        .response_json_schema
        .clone()
        .or(config.private_response_json_schema.clone())
        .or(config
            .response_schema
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?);
    let format = if let Some(schema) = schema {
        Some(openai::ResponseFormat::JsonSchema(
            openai::JsonSchemaResponseFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                name: "gemini_response".into(),
                schema: object(schema)?,
                description: None,
                strict: None,
                rest: Default::default(),
            },
        ))
    } else {
        match config.response_mime_type.as_ref() {
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson
                | gemini::ResponseMimeTypeKnown::TextXEnum,
            )) => Some(openai::ResponseFormat::JsonObject(
                openai::JsonObjectResponseFormat {
                    type_: openai::JsonObjectResponseFormatType::JsonObject,
                    rest: Default::default(),
                },
            )),
            Some(gemini::ResponseMimeType::Known(gemini::ResponseMimeTypeKnown::TextPlain))
            | None => None,
            Some(other) => {
                return Err(TransformError::unsupported(
                    "Gemini responseMimeType",
                    serde_json::to_string(other)?,
                ));
            }
        }
    };
    Ok(format.map(|format| openai::TextConfig {
        format: Some(format),
        verbosity: None,
        rest: Default::default(),
    }))
}

fn nonnegative(value: i32) -> Result<u32, TransformError> {
    u32::try_from(value)
        .map_err(|_| TransformError::shape("Gemini request", "negative integer setting"))
}

fn object(value: serde_json::Value) -> Result<openai::JsonSchema, TransformError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("Gemini response schema", "expected an object"))
}
