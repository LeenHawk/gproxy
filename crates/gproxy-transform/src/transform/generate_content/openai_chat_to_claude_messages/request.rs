use std::collections::BTreeMap;

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{
    chat_assistant_content_to_claude_blocks, chat_content_to_claude_blocks,
    chat_text_content_to_claude_blocks, chat_text_content_to_text_and_cache,
    mid_conversation_system_text_block, push_claude_block, push_claude_blocks, system_prompt,
    text_block,
};
use super::tools::{
    chat_tool_call_to_claude, chat_tool_choice_to_claude, chat_tools_to_claude,
    default_web_search_tool, normalized_tool_id, parse_json_object, tool_use_block,
};

pub fn request(
    input: openai::ChatCompletionRequest,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    // Resolved up front: the pipeline patches the upstream model into the body
    // BEFORE this transform, so model-conditional conversion sees the real
    // target model, not the inbound alias.
    let model = common::openai_model_string(input.model);
    let explicit_cache_mode =
        common::openai_cache_mode_is_explicit(input.prompt_cache_options.as_ref());
    let mid_conv_supported = common::supports_mid_conv_system(&model);
    let mut messages = Vec::new();
    let mut system_blocks = Vec::new();
    let mut seen_non_system = false;
    let mut tool_ids = BTreeMap::new();

    for (index, message) in input.messages.into_iter().enumerate() {
        match message {
            openai::ChatCompletionMessageParam::Developer { content, .. }
            | openai::ChatCompletionMessageParam::System { content, .. } => {
                let blocks = chat_text_content_to_claude_blocks(content);
                if blocks.is_empty() {
                    continue;
                }
                if !seen_non_system {
                    system_blocks.extend(blocks);
                } else if mid_conv_supported {
                    for block in blocks {
                        if let claude::ContentBlockParam::Text(block) = block {
                            push_claude_block(
                                &mut messages,
                                claude::MessageRole::Known(claude::MessageRoleKnown::User),
                                mid_conversation_system_text_block(block),
                            );
                        }
                    }
                } else {
                    // Pre-Opus-4.8 models reject mid_conv_system ("role
                    // 'system' is not supported on this model") — downgrade to
                    // a plain assistant turn.
                    push_claude_blocks(
                        &mut messages,
                        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                        blocks,
                    );
                }
            }
            openai::ChatCompletionMessageParam::User { content, .. } => {
                seen_non_system = true;
                let blocks = chat_content_to_claude_blocks(content);
                push_claude_blocks(
                    &mut messages,
                    claude::MessageRole::Known(claude::MessageRoleKnown::User),
                    blocks,
                );
            }
            openai::ChatCompletionMessageParam::Assistant {
                content,
                function_call,
                refusal,
                tool_calls,
                ..
            } => {
                seen_non_system = true;
                let mut blocks = Vec::new();
                if let Some(content) = content {
                    blocks.extend(chat_assistant_content_to_claude_blocks(content));
                }
                if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
                    blocks.push(text_block(refusal));
                }
                if let Some(function_call) = function_call {
                    let id = normalized_tool_id(format!("function_call_{index}"), &mut tool_ids);
                    blocks.push(tool_use_block(
                        id,
                        function_call.name,
                        parse_json_object(function_call.arguments),
                    ));
                }
                if let Some(tool_calls) = tool_calls {
                    for call in tool_calls {
                        blocks.push(chat_tool_call_to_claude(call, &mut tool_ids));
                    }
                }
                push_claude_blocks(
                    &mut messages,
                    claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                    blocks,
                );
            }
            openai::ChatCompletionMessageParam::Tool {
                content,
                tool_call_id,
                ..
            } => {
                seen_non_system = true;
                let (content, cache_control) = chat_text_content_to_text_and_cache(content);
                let id = normalized_tool_id(tool_call_id, &mut tool_ids);
                push_claude_block(
                    &mut messages,
                    claude::MessageRole::Known(claude::MessageRoleKnown::User),
                    claude::ContentBlockParam::ToolResult(claude::ToolResultBlock {
                        tool_use_id: id,
                        type_: claude::ToolResultBlockType::ToolResult,
                        cache_control,
                        content: Some(claude::ToolResultContent::Text(content)),
                        is_error: None,
                    }),
                );
            }
            openai::ChatCompletionMessageParam::Function { content, name, .. } => {
                seen_non_system = true;
                let text = if content.is_empty() {
                    format!("function:{name}")
                } else {
                    format!("function:{name}\n{content}")
                };
                push_claude_block(
                    &mut messages,
                    claude::MessageRole::Known(claude::MessageRoleKnown::User),
                    text_block(text),
                );
            }
        }
    }

    let max_tokens = common::merge_openai_max_tokens(input.max_completion_tokens, input.max_tokens)
        .map(u64::from)
        .unwrap_or(common::DEFAULT_CLAUDE_MAX_TOKENS);
    let output_config = chat_output_config(input.response_format, input.verbosity);
    let metadata = input
        .user
        .or_else(|| {
            input
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("user_id").cloned())
        })
        .map(|user_id| claude::Metadata {
            user_id: Some(user_id),
            extra: Default::default(),
        });

    let mut tools = input.tools.map(chat_tools_to_claude).unwrap_or_default();
    if input.web_search_options.is_some() {
        tools.push(default_web_search_tool());
    }

    #[allow(deprecated)]
    let output = claude::CreateMessageRequestBody {
        model: model.into(),
        messages,
        max_tokens,
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
        speed: openai_service_tier_to_claude_speed(input.service_tier),
        stop_sequences: common::openai_stop_to_vec(input.stop),
        stream: input.stream,
        system: system_prompt(system_blocks),
        temperature: input.temperature,
        thinking: common::openai_reasoning_to_claude(input.reasoning_effort),
        tool_choice: chat_tool_choice_to_claude(input.tool_choice, input.parallel_tool_calls),
        tools: if tools.is_empty() { None } else { Some(tools) },
        top_k: None,
        top_p: input.top_p,
        user_profile_id: None,
        extra: Default::default(),
    };
    common::apply_openai_cache_policy(output, explicit_cache_mode)
}

fn chat_output_config(
    response_format: Option<openai::ChatResponseFormat>,
    verbosity: Option<openai::Verbosity>,
) -> Option<claude::OutputConfig> {
    let format = common::chat_response_format_to_claude(response_format);
    let effort = verbosity.map(|verbosity| match verbosity {
        openai::Verbosity::Low => claude::OutputEffort::Known(claude::OutputEffortKnown::Low),
        openai::Verbosity::Medium => claude::OutputEffort::Known(claude::OutputEffortKnown::Medium),
        openai::Verbosity::High => claude::OutputEffort::Known(claude::OutputEffortKnown::High),
    });
    if effort.is_none() && format.is_none() {
        None
    } else {
        Some(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            extra: Default::default(),
        })
    }
}

fn openai_service_tier_to_claude_speed(
    service_tier: Option<openai::ServiceTier>,
) -> Option<claude::Speed> {
    match service_tier {
        Some(openai::ServiceTier::Priority) => Some(claude::Speed::Known(claude::SpeedKnown::Fast)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChatCompletions,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
        )
    }

    #[test]
    fn explicit_cache_mode_keeps_only_final_four_breakpoints() {
        let parts = (1..=6)
            .map(|index| {
                json!({
                    "type": "text",
                    "text": format!("block-{index}"),
                    "prompt_cache_breakpoint": {"mode": "explicit"}
                })
            })
            .collect::<Vec<_>>();
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "prompt_cache_options": {"mode": "explicit", "ttl": "30m"},
            "messages": [{"role": "user", "content": parts}]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert!(output.get("cache_control").is_none());
        let blocks = output["messages"][0]["content"].as_array().unwrap();
        assert!(blocks[0].get("cache_control").is_none());
        assert!(blocks[1].get("cache_control").is_none());
        for block in &blocks[2..] {
            assert_eq!(block["cache_control"]["type"], "ephemeral");
            assert!(block["cache_control"].get("ttl").is_none());
        }
    }

    #[test]
    fn implicit_cache_mode_uses_root_and_keeps_final_three_explicit_breakpoints() {
        let parts = (1..=4)
            .map(|index| {
                json!({
                    "type": "text",
                    "text": format!("block-{index}"),
                    "prompt_cache_breakpoint": {"mode": "explicit"}
                })
            })
            .collect::<Vec<_>>();
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "messages": [{"role": "user", "content": parts}]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["cache_control"]["type"], "ephemeral");
        let blocks = output["messages"][0]["content"].as_array().unwrap();
        assert!(blocks[0].get("cache_control").is_none());
        assert!(
            blocks[1..]
                .iter()
                .all(|block| block.get("cache_control").is_some())
        );
    }

    /// Regression: pre-Opus-4.8 models reject `mid_conv_system` ("role 'system'
    /// is not supported on this model") — mid-conversation system messages must
    /// become assistant turns there, and stay `mid_conv_system` on 4.8+.
    #[test]
    fn mid_conversation_system_downgrades_for_pre_opus_48() {
        let convert = |model: &str| {
            let input: openai::ChatCompletionRequest = serde_json::from_value(serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": "sys"},
                    {"role": "user", "content": "hi"},
                    {"role": "system", "content": "mid"},
                ],
            }))
            .unwrap();
            request(input, &ctx()).unwrap()
        };

        let old = convert("claude-sonnet-4-5");
        let last = old.messages.last().unwrap();
        assert_eq!(
            last.role,
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant)
        );

        let new = convert("claude-opus-4-8");
        let last = new.messages.last().unwrap();
        assert_eq!(
            last.role,
            claude::MessageRole::Known(claude::MessageRoleKnown::User)
        );
        let claude::StringOrArray::Array(blocks) = &last.content else {
            panic!("expected block content");
        };
        assert!(matches!(
            blocks.last().unwrap(),
            claude::ContentBlockParam::MidConversationSystem(_)
        ));
    }
}
