use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{claude_messages_to_gemini_contents, claude_system_to_gemini};
use super::tools::{claude_tool_choice_to_gemini, claude_tools_to_gemini};

#[allow(deprecated)]
pub fn request(
    input: claude::CreateMessageRequestBody,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let output_format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or(input.output_format);

    Ok(gemini::GenerateContentRequest {
        model: Some(common::claude_model_string(input.model)),
        contents: claude_messages_to_gemini_contents(input.messages),
        tools: input.tools.map(claude_tools_to_gemini).unwrap_or_default(),
        tool_config: claude_tool_choice_to_gemini(input.tool_choice),
        safety_settings: Vec::new(),
        system_instruction: claude_system_to_gemini(input.system),
        generation_config: generation_config(
            input.max_tokens,
            input.stop_sequences,
            input.temperature,
            input.top_p,
            input.top_k,
            input.thinking,
            output_format,
        ),
        cached_content: None,
        service_tier: common::claude_service_tier_to_gemini(input.service_tier),
        store: None,
        extra: Default::default(),
    })
}

fn generation_config(
    max_tokens: u64,
    stop_sequences: Option<Vec<String>>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<i64>,
    thinking: Option<claude::ThinkingConfig>,
    output_format: Option<claude::JsonSchemaFormat>,
) -> Option<gemini::GenerationConfig> {
    let mut config = gemini::GenerationConfig {
        stop_sequences: stop_sequences.unwrap_or_default(),
        max_output_tokens: Some(u64_to_i32(max_tokens)),
        temperature,
        top_p,
        top_k: top_k.map(i64_to_i32),
        thinking_config: common::claude_thinking_to_gemini(thinking),
        ..Default::default()
    };
    if let Some(format) = output_format {
        config.response_mime_type = Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::ApplicationJson,
        ));
        config.response_json_schema = Some(serde_json::to_value(format.schema).unwrap_or_default());
    }
    Some(config)
}

fn u64_to_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn i64_to_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or_else(|_| {
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}
