use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::super::openai_chat_to_claude_messages::tools::{
    chat_tool_choice_to_claude, chat_tools_to_claude, default_web_search_tool,
    tools_activate_programmatic_calling,
};
use super::super::openai_responses_to_openai_chat::tools::{
    response_tool_choice_to_chat_tool_choice, response_tools_for_chat,
};
use crate::transform::compact::openai_to_claude::openai_input_to_claude_messages;

pub fn request(
    input: openai::ResponseCreateRequest,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let model = input
        .model
        .map(common::openai_model_string)
        .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned());
    let system_role = if common::supports_mid_conv_system(&model) {
        claude::MessageRole::Known(claude::MessageRoleKnown::System)
    } else {
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant)
    };
    let implicit_cache_mode =
        common::openai_cache_mode_is_implicit(input.prompt_cache_options.as_ref());
    let effort = input
        .reasoning
        .as_ref()
        .and_then(|reasoning| reasoning.effort.clone());
    let format = common::response_text_config_to_claude(input.text);
    let output_config = match (common::openai_effort_to_claude(effort.clone()), format) {
        (None, None) => None,
        (effort, format) => Some(crate::protocol::wire!(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            extra: Default::default(),
        })),
    };
    let metadata = input
        .user
        .or_else(|| {
            input
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("user_id").cloned())
        })
        .map(|user_id| {
            crate::protocol::wire!(claude::Metadata {
                user_id: Some(user_id),
                extra: Default::default(),
            })
        });

    let response_tools = response_tools_for_chat(input.tools);
    let mut tools = response_tools
        .tools
        .map(chat_tools_to_claude)
        .unwrap_or_default();
    if response_tools.web_search_options.is_some() {
        tools.push(default_web_search_tool());
    }
    let parallel_tool_calls = match input.parallel_tool_calls {
        Some(false) if tools_activate_programmatic_calling(&tools) => None,
        other => other,
    };
    let tool_choice = chat_tool_choice_to_claude(
        response_tool_choice_to_chat_tool_choice(input.tool_choice),
        parallel_tool_calls,
    );

    #[allow(deprecated)]
    let output = crate::protocol::wire!(claude::CreateMessageRequestBody {
        model: claude::ClaudeModel::Unknown(model),
        messages: openai_input_to_claude_messages(input.input, system_role),
        max_tokens: input
            .max_output_tokens
            .map(u64::from)
            .unwrap_or(common::DEFAULT_CLAUDE_MAX_TOKENS),
        cache_control: None,
        container: None,
        context_management: None,
        diagnostics: None,
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata,
        output_config,
        output_format: None,
        service_tier: common::openai_service_tier_to_claude(input.service_tier.clone()),
        speed: common::openai_service_tier_to_claude_speed(input.service_tier),
        stop_sequences: None,
        stream: input.stream,
        system: input.instructions.map(claude::SystemPrompt::String),
        temperature: input.temperature,
        thinking: common::openai_reasoning_to_claude(effort),
        tool_choice,
        tools: (!tools.is_empty()).then_some(tools),
        top_k: None,
        top_p: input.top_p,
        user_profile_id: None,
        extra: Default::default(),
    });
    common::apply_openai_cache_policy(output, implicit_cache_mode)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey, claude, openai};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
        )
    }

    #[test]
    fn explicit_responses_cache_keeps_final_four_breakpoints_for_claude() {
        let input = crate::protocol::wire!(openai::ResponseCreateRequest {
            model: Some(openai::OpenAiModelId::Unknown("claude-sonnet-4-6".to_owned())),
            prompt_cache_options: Some(crate::protocol::wire!(openai::PromptCacheOptions {
                mode: Some(openai::PromptCacheMode::Explicit),
                ttl: Some(openai::PromptCacheTtl::ThirtyMinutes),
                extra: Default::default(),
            })),
            input: Some(serde_json::from_value(json!([
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "1", "prompt_cache_breakpoint": {"mode": "explicit"}},
                    {"type": "input_text", "text": "2", "prompt_cache_breakpoint": {"mode": "explicit"}},
                    {"type": "input_text", "text": "3", "prompt_cache_breakpoint": {"mode": "explicit"}},
                    {"type": "input_text", "text": "4", "prompt_cache_breakpoint": {"mode": "explicit"}},
                    {"type": "input_text", "text": "5", "prompt_cache_breakpoint": {"mode": "explicit"}}
                ]}
            ])).unwrap()),
            ..Default::default()
        });

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert!(output.get("cache_control").is_none());
        let blocks = output["messages"][0]["content"].as_array().unwrap();
        assert!(blocks[0].get("cache_control").is_none());
        assert!(
            blocks[1..]
                .iter()
                .all(|block| block.get("cache_control").is_some())
        );
    }

    #[test]
    fn apply_patch_result_reaches_claude_as_tool_result() {
        let input = crate::protocol::wire!(openai::ResponseCreateRequest {
            model: Some(openai::OpenAiModelId::Unknown("test-model".to_owned())),
            input: Some(openai::ResponseInput::Items(vec![
                serde_json::from_value(json!({
                    "type": "apply_patch_call",
                    "call_id": "call_patch",
                    "operation": {
                        "type": "update_file",
                        "path": "src/lib.rs",
                        "diff": "*** Begin Patch\n*** End Patch"
                    },
                    "status": "completed"
                }))
                .unwrap(),
                serde_json::from_value(json!({
                    "type": "apply_patch_call_output",
                    "call_id": "call_patch",
                    "status": "failed",
                    "output": "Model tried to call unavailable tool 'apply_patch'. Available tools: edit."
                }))
                .unwrap(),
            ])),
            ..Default::default()
        });

        let out = request(input, &ctx()).unwrap();
        assert_eq!(out.messages.len(), 2);

        let claude::MessageParam { content, .. } = &out.messages[0];
        let claude::StringOrArray::Array(blocks) = content else {
            panic!("expected assistant blocks");
        };
        let claude::ContentBlockParam::ToolUse(tool_use) = &blocks[0] else {
            panic!("expected apply_patch tool_use");
        };
        assert_eq!(tool_use.id, "toolu_call_patch");
        assert_eq!(tool_use.name, "apply_patch");
        assert_eq!(
            tool_use.input.get("type").and_then(|v| v.as_str()),
            Some("update_file")
        );

        let claude::MessageParam { content, .. } = &out.messages[1];
        let claude::StringOrArray::Array(blocks) = content else {
            panic!("expected user blocks");
        };
        let claude::ContentBlockParam::ToolResult(result) = &blocks[0] else {
            panic!("expected apply_patch tool_result");
        };
        assert_eq!(result.tool_use_id, "toolu_call_patch");
        assert_eq!(
            result.content,
            Some(claude::ToolResultContent::Text(
                "Model tried to call unavailable tool 'apply_patch'. Available tools: edit."
                    .to_owned()
            ))
        );
    }

    #[test]
    fn maps_encrypted_reasoning_directly_to_claude_thinking() {
        let input: openai::ResponseCreateRequest = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "input": [{
                "type": "reasoning",
                "summary": [],
                "content": [{"type": "reasoning_text", "text": "hidden"}],
                "encrypted_content": "ciphertext"
            }]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["messages"][0]["content"][0]["type"], "thinking");
        assert_eq!(output["messages"][0]["content"][0]["thinking"], "hidden");
        assert_eq!(
            output["messages"][0]["content"][0]["signature"],
            "ciphertext"
        );
    }
}
