use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{gemini_content_to_claude_system, gemini_contents_to_claude_messages};
use super::tools::{gemini_tool_config_to_claude, gemini_tools_to_claude};

#[allow(deprecated)]
pub fn request(
    input: gemini::GenerateContentRequest,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let generation_config = input.generation_config;
    let output_format = generation_config
        .as_ref()
        .and_then(|config| config.response_json_schema.clone())
        .and_then(value_to_claude_output_format);

    Ok(claude::CreateMessageRequestBody {
        model: input.model.unwrap_or_default().into(),
        messages: gemini_contents_to_claude_messages(input.contents),
        max_tokens: generation_config
            .as_ref()
            .and_then(|config| config.max_output_tokens)
            .map(i32_to_u64)
            .unwrap_or(common::DEFAULT_CLAUDE_MAX_TOKENS),
        cache_control: None,
        container: None,
        context_management: None,
        diagnostics: None,
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: output_format.map(|format| claude::OutputConfig {
            effort: None,
            format: Some(format),
            task_budget: None,
            extra: Default::default(),
        }),
        output_format: None,
        service_tier: common::gemini_service_tier_to_claude(input.service_tier),
        speed: None,
        stop_sequences: generation_config
            .as_ref()
            .map(|config| config.stop_sequences.clone())
            .filter(|stop| !stop.is_empty()),
        stream: None,
        system: input
            .system_instruction
            .and_then(gemini_content_to_claude_system),
        temperature: generation_config
            .as_ref()
            .and_then(|config| config.temperature),
        thinking: common::gemini_thinking_to_claude(
            generation_config
                .as_ref()
                .and_then(|config| config.thinking_config.as_ref()),
        ),
        tool_choice: gemini_tool_config_to_claude(input.tool_config),
        tools: {
            let tools = gemini_tools_to_claude(input.tools);
            (!tools.is_empty()).then_some(tools)
        },
        top_k: generation_config
            .as_ref()
            .and_then(|config| config.top_k)
            .map(i64::from),
        top_p: generation_config.as_ref().and_then(|config| config.top_p),
        user_profile_id: None,
        extra: Default::default(),
    })
}

fn value_to_claude_output_format(value: serde_json::Value) -> Option<claude::JsonSchemaFormat> {
    let serde_json::Value::Object(map) = value else {
        return None;
    };
    Some(claude::JsonSchemaFormat {
        type_: claude::JsonSchemaFormatType::Known(claude::JsonSchemaFormatTypeKnown::JsonSchema),
        schema: map.into_iter().collect(),
        extra: Default::default(),
    })
}

fn i32_to_u64(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}
