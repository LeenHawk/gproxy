use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{claude_messages_to_gemini_contents, claude_system_to_gemini};
use super::tools::{claude_tool_choice_to_gemini, claude_tools_to_gemini};

#[allow(deprecated)]
pub fn request(
    mut input: claude::CreateMessageRequestBody,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    crate::transform::common::apply_claude_message_controls(
        &mut input.messages,
        &mut input.output_config,
    );
    let output_format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or(input.output_format);
    let output_effort = input
        .output_config
        .as_ref()
        .and_then(|config| config.effort.clone());

    Ok(crate::protocol::wire!(gemini::GenerateContentRequest {
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
            (input.thinking, output_effort),
            output_format,
        ),
        cached_content: None,
        service_tier: common::claude_speed_to_gemini(input.speed)
            .or_else(|| common::claude_service_tier_to_gemini(input.service_tier)),
        store: None,
        extra: Default::default(),
    }))
}

fn generation_config(
    max_tokens: u64,
    stop_sequences: Option<Vec<String>>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<i64>,
    reasoning: (Option<claude::ThinkingConfig>, Option<claude::OutputEffort>),
    output_format: Option<claude::JsonSchemaFormat>,
) -> Option<gemini::GenerationConfig> {
    let (thinking, effort) = reasoning;
    let mut thinking_config = common::claude_thinking_to_gemini(thinking);
    if let Some(level) = common::claude_effort_to_gemini(effort) {
        thinking_config
            .get_or_insert_with(Default::default)
            .thinking_level = Some(level);
    }
    let mut config = crate::protocol::wire!(gemini::GenerationConfig {
        stop_sequences: stop_sequences.unwrap_or_default(),
        max_output_tokens: Some(u64_to_i32(max_tokens)),
        temperature,
        top_p,
        top_k: top_k.map(i64_to_i32),
        thinking_config,
        ..Default::default()
    });
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn correlates_tool_result_name_and_preserves_signature() {
        let input = serde_json::from_value(json!({
            "model": "gemini-flash-latest",
            "max_tokens": 64,
            "messages": [
                {"role": "assistant", "content": [{
                    "type": "tool_use",
                    "id": "call_1",
                    "name": "get_magic",
                    "input": {"value": "ok"},
                    "caller": {"type": "direct", "thought_signature": "ciphertext"}
                }]},
                {"role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call_1",
                    "content": "ok"
                }]}
            ]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
        );

        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        assert_eq!(
            output["contents"][0]["parts"][0]["functionCall"]["id"],
            "call_1"
        );
        assert_eq!(
            output["contents"][0]["parts"][0]["functionCall"]["name"],
            "get_magic"
        );
        assert_eq!(
            output["contents"][0]["parts"][0]["thoughtSignature"],
            "ciphertext"
        );
        assert_eq!(
            output["contents"][1]["parts"][0]["functionResponse"]["id"],
            "call_1"
        );
        assert_eq!(
            output["contents"][1]["parts"][0]["functionResponse"]["name"],
            "get_magic"
        );
    }
}
