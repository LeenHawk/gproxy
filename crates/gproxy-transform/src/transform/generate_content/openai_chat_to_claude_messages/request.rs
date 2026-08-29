use std::collections::BTreeMap;

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{
    chat_assistant_content_to_claude_blocks, chat_content_to_claude_blocks,
    chat_text_content_to_claude_blocks, chat_text_content_to_text_and_cache, push_claude_block,
    push_claude_blocks, system_prompt, text_block,
};
use super::tools::{
    chat_tool_call_to_claude, chat_tool_choice_to_claude, chat_tools_to_claude,
    default_web_search_tool, normalized_tool_id, parse_json_object, tool_use_block,
    tools_activate_programmatic_calling,
};

pub fn request(
    input: openai::ChatCompletionRequest,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    // Resolved up front: the pipeline patches the upstream model into the body
    // BEFORE this transform, so model-conditional conversion sees the real
    // target model, not the inbound alias.
    let model = common::openai_model_string(input.model);
    let implicit_cache_mode =
        common::openai_cache_mode_is_implicit(input.prompt_cache_options.as_ref());
    let mid_conv_supported = common::supports_mid_conv_system(&model);
    // Index of the last non-system message. System/developer messages AFTER it
    // form the trailing run: when downgrading (pre-Opus-4.8), those must become
    // user turns — a trailing assistant turn is prefill, which newer models
    // reject and which no OpenAI client means by a trailing system message.
    let last_non_system_index = input.messages.iter().rposition(|message| {
        !matches!(
            message,
            openai::ChatCompletionMessageParam::Developer { .. }
                | openai::ChatCompletionMessageParam::System { .. }
        )
    });
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
                    push_claude_blocks(
                        &mut messages,
                        claude::MessageRole::Known(claude::MessageRoleKnown::System),
                        blocks,
                    );
                } else {
                    // Pre-Opus-4.8 models reject system-role turns — downgrade
                    // to a plain assistant turn. Trailing system messages become
                    // user turns instead: ending on assistant would be an accidental
                    // prefill (rejected by models without prefill support, e.g. Opus 4.6+).
                    let role = if last_non_system_index.is_some_and(|last| index > last) {
                        claude::MessageRoleKnown::User
                    } else {
                        claude::MessageRoleKnown::Assistant
                    };
                    push_claude_blocks(&mut messages, claude::MessageRole::Known(role), blocks);
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
                    claude::ContentBlockParam::ToolResult(crate::protocol::wire!(
                        claude::ToolResultBlock {
                            tool_use_id: id,
                            type_: claude::ToolResultBlockType::ToolResult,
                            cache_control,
                            content: Some(claude::ToolResultContent::Text(content)),
                            is_error: None,
                        }
                    )),
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
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        }
    }

    let max_tokens = common::merge_openai_max_tokens(input.max_completion_tokens, input.max_tokens)
        .map(u64::from)
        .unwrap_or(common::DEFAULT_CLAUDE_MAX_TOKENS);
    let output_config = chat_output_config(input.response_format, input.reasoning_effort.clone());
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

    let mut tools = input.tools.map(chat_tools_to_claude).unwrap_or_default();
    if input.web_search_options.is_some() {
        tools.push(default_web_search_tool());
    }

    // `parallel_tool_calls: false` maps to `disable_parallel_tool_use: true`, which
    // Anthropic rejects when the request activates programmatic tool calling — drop
    // it rather than fail the whole request (see `tools_activate_programmatic_calling`).
    let parallel_tool_calls = match input.parallel_tool_calls {
        Some(false) if tools_activate_programmatic_calling(&tools) => None,
        other => other,
    };

    #[allow(deprecated)]
    let output = crate::protocol::wire!(claude::CreateMessageRequestBody {
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
        speed: common::openai_service_tier_to_claude_speed(input.service_tier),
        stop_sequences: common::openai_stop_to_vec(input.stop),
        stream: input.stream,
        system: system_prompt(system_blocks),
        temperature: input.temperature,
        thinking: common::openai_reasoning_to_claude(input.reasoning_effort),
        tool_choice: chat_tool_choice_to_claude(input.tool_choice, parallel_tool_calls),
        tools: if tools.is_empty() { None } else { Some(tools) },
        top_k: None,
        top_p: input.top_p,
        user_profile_id: None,
        extra: Default::default(),
    });
    common::apply_openai_cache_policy(output, implicit_cache_mode)
}

fn chat_output_config(
    response_format: Option<openai::ChatResponseFormat>,
    reasoning_effort: Option<openai::ReasoningEffort>,
) -> Option<claude::OutputConfig> {
    let format = common::chat_response_format_to_claude(response_format);
    let effort = common::openai_effort_to_claude(reasoning_effort);
    if effort.is_none() && format.is_none() {
        None
    } else {
        Some(crate::protocol::wire!(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            extra: Default::default(),
        }))
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
    fn maps_openai_fast_service_tiers_to_claude_speed() {
        for tier in [
            openai::ServiceTier::Fast,
            openai::ServiceTier::Priority,
            openai::ServiceTier::Ultrafast,
        ] {
            assert_eq!(
                common::openai_service_tier_to_claude_speed(Some(tier)),
                Some(claude::Speed::Known(claude::SpeedKnown::Fast))
            );
        }
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
            "prompt_cache_options": {"mode": "implicit"},
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

    #[test]
    fn unspecified_cache_mode_does_not_add_root_cache_control() {
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert!(output.get("cache_control").is_none());
    }

    /// Regression: pre-Opus-4.8 models reject system-role turns, while wrapping
    /// an isolated 4.8 system turn in a user `mid_conv_system` block makes that
    /// user turn fail the non-empty-content validation.
    #[test]
    fn mid_conversation_system_uses_system_role_for_opus_48() {
        let convert = |model: &str| {
            let input: openai::ChatCompletionRequest = serde_json::from_value(serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": "sys"},
                    {"role": "user", "content": "hi"},
                    {"role": "assistant", "content": "first reply"},
                    {"role": "system", "content": "mid"},
                    {"role": "assistant", "content": "second reply"},
                    {"role": "user", "content": "more"},
                ],
            }))
            .unwrap();
            request(input, &ctx()).unwrap()
        };

        let old = convert("claude-sonnet-4-5");
        assert_eq!(old.messages.len(), 3);
        assert_eq!(
            old.messages[1].role,
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant)
        );

        let new = convert("claude-opus-4-8");
        assert_eq!(new.messages.len(), 5);
        assert_eq!(
            new.messages[2].role,
            claude::MessageRole::Known(claude::MessageRoleKnown::System)
        );
        let output = serde_json::to_value(new).unwrap();
        let roles: Vec<&str> = output["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(
            roles,
            vec!["user", "assistant", "system", "assistant", "user"]
        );
        assert_eq!(output["messages"][2]["content"][0]["type"], "text");
        assert_eq!(output["messages"][2]["content"][0]["text"], "mid");
    }

    #[test]
    fn maps_reasoning_effort_to_claude_output_effort() {
        let input = serde_json::from_value(json!({
            "model": "claude-opus-5",
            "messages": [{"role": "user", "content": "solve it"}],
            "reasoning_effort": "max",
            "verbosity": "low"
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["output_config"]["effort"], "max");
        assert_eq!(output["thinking"]["type"], "adaptive");
    }

    /// Regression: Anthropic rejects `disable_parallel_tool_use: true` together with
    /// programmatic tool calling (activated here by the 2026-generation web_search
    /// server tool), so the flag must be dropped — but kept when no PTC tool is present.
    #[test]
    fn drops_disable_parallel_tool_use_when_web_search_activates_ptc() {
        let base = json!({
            "model": "claude-opus-4-8",
            "messages": [{"role": "user", "content": "hi"}],
            "parallel_tool_calls": false,
            "tool_choice": "auto",
            "tools": [{"type": "function", "function": {"name": "echo", "parameters": {"type": "object"}}}]
        });

        let mut with_search = base.clone();
        with_search["web_search_options"] = json!({});
        let input = serde_json::from_value(with_search).unwrap();
        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert!(
            output["tool_choice"]
                .get("disable_parallel_tool_use")
                .is_none(),
            "{}",
            output["tool_choice"]
        );

        let input = serde_json::from_value(base).unwrap();
        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["tool_choice"]["disable_parallel_tool_use"], true);
    }

    /// Regression: a developer message before the next user message must remain
    /// in its original position as a system turn.
    #[test]
    fn mid_conversation_system_before_user_preserves_order() {
        let input = serde_json::from_value(json!({
            "model": "claude-opus-4-8",
            "messages": [
                {"role": "user", "content": "turn one"},
                {"role": "assistant", "content": "reply one"},
                {"role": "developer", "content": "<model_switch>switch</model_switch>"},
                {"role": "user", "content": "turn two"}
            ]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        let roles: Vec<&str> = output["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "assistant", "system", "user"]);
        assert_eq!(
            output["messages"][2]["content"][0]["text"],
            "<model_switch>switch</model_switch>"
        );
    }

    /// Control: a trailing mid-conversation system message is also a system turn,
    /// not a user turn that Anthropic can reject as empty.
    #[test]
    fn trailing_mid_conversation_system_uses_system_role() {
        let input = serde_json::from_value(json!({
            "model": "claude-opus-4-8",
            "messages": [
                {"role": "user", "content": "turn one"},
                {"role": "assistant", "content": "reply one"},
                {"role": "developer", "content": "note"}
            ]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["messages"][2]["role"], "system");
        assert_eq!(output["messages"][2]["content"][0]["text"], "note");
    }
}
