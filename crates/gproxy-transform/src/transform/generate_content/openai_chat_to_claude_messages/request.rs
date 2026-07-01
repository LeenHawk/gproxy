use std::collections::BTreeMap;

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{
    chat_assistant_content_to_claude_blocks, chat_content_to_claude_blocks,
    chat_text_content_to_text, mid_conversation_system_block, push_claude_block,
    push_claude_blocks, system_prompt, text_block,
};
use super::tools::{
    chat_tool_call_to_claude, chat_tool_choice_to_claude, chat_tools_to_claude,
    default_web_search_tool, normalized_tool_id, parse_json_object, tool_use_block,
};

pub fn request(
    input: openai::ChatCompletionRequest,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let mut messages = Vec::new();
    let mut system_blocks = Vec::new();
    let mut seen_non_system = false;
    let mut tool_ids = BTreeMap::new();

    for (index, message) in input.messages.into_iter().enumerate() {
        match message {
            openai::ChatCompletionMessageParam::Developer { content, .. }
            | openai::ChatCompletionMessageParam::System { content, .. } => {
                let text = chat_text_content_to_text(content);
                if text.is_empty() {
                    continue;
                }
                if seen_non_system {
                    push_claude_block(
                        &mut messages,
                        claude::MessageRole::Known(claude::MessageRoleKnown::User),
                        mid_conversation_system_block(text),
                    );
                } else {
                    system_blocks.push(text_block(text));
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
                let content = chat_text_content_to_text(content);
                let id = normalized_tool_id(tool_call_id, &mut tool_ids);
                push_claude_block(
                    &mut messages,
                    claude::MessageRole::Known(claude::MessageRoleKnown::User),
                    claude::ContentBlockParam::ToolResult(claude::ToolResultBlock {
                        tool_use_id: id,
                        type_: claude::ToolResultBlockType::ToolResult,
                        cache_control: None,
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
    Ok(claude::CreateMessageRequestBody {
        model: common::openai_model_string(input.model).into(),
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
    })
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
