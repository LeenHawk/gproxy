use crate::protocol::{claude, openai};

use super::super::tools::{
    claude_response_tool_use_to_chat_tool_call, claude_tool_use_to_chat_tool_call,
};
use super::cache::{breakpoint_for_text, warn_unrepresentable_cache_control};

pub(in super::super) fn claude_blocks_to_assistant_message(
    blocks: Vec<claude::ContentBlockParam>,
) -> openai::ChatCompletionMessageParam {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => {
                let prompt_cache_breakpoint = breakpoint_for_text(
                    &block.text,
                    block.cache_control,
                    "OpenAI Chat assistant message",
                );
                content_parts.push(openai::ChatAssistantContentPart::Text {
                    text: block.text,
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::Thinking(block) => {
                content_parts.push(openai::ChatAssistantContentPart::Text {
                    text: block.thinking,
                    prompt_cache_breakpoint: None,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::ToolUse(block) => {
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
                tool_calls.push(claude_tool_use_to_chat_tool_call(block));
            }
            claude::ContentBlockParam::ServerToolUse(block) => {
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "server_tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: serde_json::to_value(block.name)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "server_tool".to_owned()),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::McpToolUse(block) => {
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "mcp_tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: format!("mcp:{}:{}", block.server_name, block.name),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            other => warn_unrepresentable_cache_control(&other, "OpenAI Chat assistant message"),
        }
    }

    openai::ChatCompletionMessageParam::Assistant {
        content: (!content_parts.is_empty())
            .then_some(openai::ChatAssistantContent::Parts(content_parts)),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        extra: Default::default(),
    }
}

pub(in super::super) fn claude_response_blocks_to_chat_message(
    blocks: Vec<claude::ContentBlock>,
) -> openai::ChatMessage {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlock::Text(block) => text_parts.push(block.text),
            claude::ContentBlock::Thinking(block) => text_parts.push(block.thinking),
            claude::ContentBlock::ToolUse(block) => {
                tool_calls.push(claude_response_tool_use_to_chat_tool_call(block));
            }
            claude::ContentBlock::ServerToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: serde_json::to_value(block.name)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "server_tool".to_owned()),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            claude::ContentBlock::McpToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: format!("mcp:{}:{}", block.server_name, block.name),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: (!text_parts.is_empty()).then(|| text_parts.join("\n")),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        extra: Default::default(),
    }
}
