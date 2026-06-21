use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{gemini_content_to_text, gemini_contents_to_chat_messages};
use super::tools::{gemini_tool_config_to_chat, gemini_tools_to_chat};

pub fn request(
    input: gemini::GenerateContentRequest,
    _: &TransformContext,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let mut messages = Vec::new();
    if let Some(system_instruction) = input.system_instruction {
        let text = gemini_content_to_text(system_instruction);
        if !text.is_empty() {
            messages.push(openai::ChatCompletionMessageParam::System {
                content: openai::ChatTextContent::Text(text),
                name: None,
                extra: Default::default(),
            });
        }
    }
    messages.extend(gemini_contents_to_chat_messages(input.contents));

    let generation_config = input.generation_config;
    let response_format = generation_config
        .as_ref()
        .and_then(gemini_response_format_to_chat);

    Ok(openai::ChatCompletionRequest {
        messages,
        model: input
            .model
            .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
            .into(),
        audio: None,
        frequency_penalty: generation_config
            .as_ref()
            .and_then(|config| config.frequency_penalty),
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: generation_config
            .as_ref()
            .and_then(|config| config.response_logprobs),
        max_completion_tokens: generation_config
            .as_ref()
            .and_then(|config| config.max_output_tokens)
            .map(i32_to_u32),
        max_tokens: None,
        metadata: None,
        modalities: None,
        moderation: None,
        n: generation_config
            .as_ref()
            .and_then(|config| config.candidate_count)
            .map(i32_to_u32),
        parallel_tool_calls: None,
        prediction: None,
        presence_penalty: generation_config
            .as_ref()
            .and_then(|config| config.presence_penalty),
        prompt_cache_key: input.cached_content,
        prompt_cache_retention: None,
        reasoning_effort: common::gemini_thinking_to_openai(
            generation_config
                .as_ref()
                .and_then(|config| config.thinking_config.as_ref()),
        ),
        response_format,
        safety_identifier: None,
        seed: generation_config.as_ref().and_then(|config| config.seed),
        service_tier: common::gemini_service_tier_to_openai(input.service_tier),
        stop: common::vec_to_openai_stop(
            generation_config
                .as_ref()
                .map(|config| config.stop_sequences.clone()),
        ),
        store: input.store,
        stream: None,
        stream_options: None,
        temperature: generation_config
            .as_ref()
            .and_then(|config| config.temperature),
        tool_choice: gemini_tool_config_to_chat(input.tool_config),
        tools: {
            let tools = gemini_tools_to_chat(input.tools);
            (!tools.is_empty()).then_some(tools)
        },
        top_logprobs: generation_config
            .as_ref()
            .and_then(|config| config.logprobs)
            .map(i32_to_u32),
        top_p: generation_config.as_ref().and_then(|config| config.top_p),
        user: None,
        verbosity: None,
        web_search_options: None,
        extra: Default::default(),
    })
}

fn gemini_response_format_to_chat(
    config: &gemini::GenerationConfig,
) -> Option<openai::ChatResponseFormat> {
    if let Some(schema) = config.response_json_schema.clone() {
        return Some(openai::ChatResponseFormat::ChatJsonSchema(
            openai::ChatJsonSchemaFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                json_schema: openai::JsonSchemaFormat {
                    name: "response".to_owned(),
                    description: None,
                    schema: value_to_json_schema(schema),
                    strict: None,
                    extra: Default::default(),
                },
                extra: Default::default(),
            },
        ));
    }
    common::gemini_response_mime_to_chat(Some(config))
}

fn value_to_json_schema(value: serde_json::Value) -> Option<openai::JsonSchema> {
    match value {
        serde_json::Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}
