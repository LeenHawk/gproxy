use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{
    claude_blocks_to_assistant_message, claude_blocks_to_user_messages, claude_content_to_text,
    claude_system_to_text, push_developer_message, push_system_message,
};
use super::tools::{claude_tool_choice_to_chat, claude_tools_to_chat};

#[allow(deprecated)]
pub fn request(
    input: claude::CreateMessageRequestBody,
    _: &TransformContext,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let mut messages = Vec::new();
    if let Some(system) = claude_system_to_text(input.system) {
        push_system_message(&mut messages, system);
    }

    for message in input.messages {
        match message.role {
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
                messages.push(match message.content {
                    claude::StringOrArray::String(text) => {
                        openai::ChatCompletionMessageParam::Assistant {
                            content: Some(openai::ChatAssistantContent::Text(text)),
                            audio: None,
                            function_call: None,
                            name: None,
                            reasoning_content: None,
                            refusal: None,
                            tool_calls: None,
                            extra: Default::default(),
                        }
                    }
                    claude::StringOrArray::Array(blocks) => {
                        claude_blocks_to_assistant_message(blocks)
                    }
                });
            }
            claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
                let text = claude_content_to_text(message.content);
                push_developer_message(&mut messages, text);
            }
            claude::MessageRole::Known(claude::MessageRoleKnown::User)
            | claude::MessageRole::Unknown(_) => match message.content {
                claude::StringOrArray::String(text) => {
                    messages.push(openai::ChatCompletionMessageParam::User {
                        content: openai::ChatContent::Text(text),
                        name: None,
                        extra: Default::default(),
                    });
                }
                claude::StringOrArray::Array(blocks) => {
                    messages.extend(claude_blocks_to_user_messages(blocks));
                }
            },
        }
    }

    let output_format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or(input.output_format);

    Ok(openai::ChatCompletionRequest {
        messages,
        model: common::claude_model_string(input.model).into(),
        audio: None,
        frequency_penalty: None,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: None,
        max_completion_tokens: Some(u64_to_u32(input.max_tokens)),
        max_tokens: None,
        metadata: None,
        modalities: None,
        moderation: None,
        n: None,
        parallel_tool_calls: input
            .tool_choice
            .as_ref()
            .and_then(claude_parallel_tool_calls),
        prediction: None,
        presence_penalty: None,
        prompt_cache_key: None,
        prompt_cache_retention: None,
        reasoning_effort: common::claude_thinking_to_openai(input.thinking),
        response_format: common::claude_output_format_to_chat(output_format),
        safety_identifier: None,
        seed: None,
        service_tier: common::claude_service_tier_to_openai(input.service_tier),
        stop: common::vec_to_openai_stop(input.stop_sequences),
        store: None,
        stream: input.stream,
        stream_options: None,
        temperature: input.temperature,
        tool_choice: claude_tool_choice_to_chat(input.tool_choice),
        tools: input
            .tools
            .map(claude_tools_to_chat)
            .filter(|tools| !tools.is_empty()),
        top_logprobs: None,
        top_p: input.top_p,
        user: input.metadata.and_then(|metadata| metadata.user_id),
        verbosity: input.output_config.and_then(|config| {
            config.effort.map(|effort| match effort {
                claude::OutputEffort::Known(claude::OutputEffortKnown::Low) => {
                    openai::Verbosity::Low
                }
                claude::OutputEffort::Known(claude::OutputEffortKnown::Medium) => {
                    openai::Verbosity::Medium
                }
                claude::OutputEffort::Known(
                    claude::OutputEffortKnown::High
                    | claude::OutputEffortKnown::XHigh
                    | claude::OutputEffortKnown::Max,
                )
                | claude::OutputEffort::Unknown(_) => openai::Verbosity::High,
            })
        }),
        web_search_options: None,
        extra: Default::default(),
    })
}

fn claude_parallel_tool_calls(choice: &claude::ToolChoice) -> Option<bool> {
    match choice {
        claude::ToolChoice::Auto(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Any(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Tool(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::None(_) | claude::ToolChoice::Unknown(_) => None,
    }
}

fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
