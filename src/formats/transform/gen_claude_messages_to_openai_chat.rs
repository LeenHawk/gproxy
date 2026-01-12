use std::collections::HashMap;

use time::OffsetDateTime;

use crate::formats::{claude, openai};

use super::{map_status_headers, ResponseParts, TransformError};

fn finish_reason_from_claude(
    reason: Option<claude::messages::BetaStopReason>,
) -> openai::chat_completions::ChatCompletionFinishReason {
    use claude::messages::BetaStopReason;
    use openai::chat_completions::ChatCompletionFinishReason;

    match reason {
        Some(BetaStopReason::EndTurn) => ChatCompletionFinishReason::Stop,
        Some(BetaStopReason::MaxTokens) => ChatCompletionFinishReason::Length,
        Some(BetaStopReason::StopSequence) => ChatCompletionFinishReason::Stop,
        Some(BetaStopReason::ToolUse) => ChatCompletionFinishReason::ToolCalls,
        Some(BetaStopReason::Refusal) => ChatCompletionFinishReason::ContentFilter,
        Some(BetaStopReason::ModelContextWindowExceeded) => ChatCompletionFinishReason::Length,
        Some(BetaStopReason::PauseTurn) | None => ChatCompletionFinishReason::Stop,
    }
}

fn text_from_blocks(blocks: &[claude::messages::BetaContentBlock]) -> String {
    let mut output = String::new();
    for block in blocks {
        if let Some(text) = block_text(block) {
            append_text(&mut output, &text);
        }
    }
    output
}

fn append_text(output: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(text);
}

fn block_text(block: &claude::messages::BetaContentBlock) -> Option<String> {
    match block {
        claude::messages::BetaContentBlock::Text(text) => Some(text.text.clone()),
        claude::messages::BetaContentBlock::Thinking(thinking) => Some(thinking.thinking.clone()),
        claude::messages::BetaContentBlock::RedactedThinking(_) => {
            Some("[redacted_thinking]".to_string())
        }
        claude::messages::BetaContentBlock::MCPToolResult(result) => match &result.content {
            claude::messages::MCPToolResultContent::Text(text) => Some(text.clone()),
            claude::messages::MCPToolResultContent::Blocks(blocks) => Some(
                blocks
                    .iter()
                    .map(|block| block.text.as_str())
                    .collect(),
            ),
        },
        claude::messages::BetaContentBlock::WebSearchToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        claude::messages::BetaContentBlock::WebFetchToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        claude::messages::BetaContentBlock::CodeExecutionToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        claude::messages::BetaContentBlock::BashCodeExecutionToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        claude::messages::BetaContentBlock::TextEditorCodeExecutionToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        claude::messages::BetaContentBlock::ToolSearchToolResult(result) => {
            serde_json::to_string(result).ok()
        }
        _ => None,
    }
}

fn server_tool_name(name: &claude::types::ServerToolName) -> &'static str {
    match name {
        claude::types::ServerToolName::WebSearch => "web_search",
        claude::types::ServerToolName::WebFetch => "web_fetch",
        claude::types::ServerToolName::CodeExecution => "code_execution",
        claude::types::ServerToolName::BashCodeExecution => "bash_code_execution",
        claude::types::ServerToolName::TextEditorCodeExecution => "text_editor_code_execution",
        claude::types::ServerToolName::ToolSearchToolRegex => "tool_search_tool_regex",
        claude::types::ServerToolName::ToolSearchToolBm25 => "tool_search_tool_bm25",
    }
}

fn tool_call_arguments(input: &HashMap<String, serde_json::Value>) -> String {
    serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string())
}

fn tool_calls_from_blocks(
    blocks: &[claude::messages::BetaContentBlock],
) -> Vec<openai::chat_completions::ChatCompletionMessageToolCall> {
    let mut calls = Vec::new();
    for block in blocks {
        match block {
            claude::messages::BetaContentBlock::ToolUse(tool) => {
                calls.push(openai::chat_completions::ChatCompletionMessageToolCall::Function {
                    id: tool.id.clone(),
                    function: openai::chat_completions::ChatCompletionToolCallFunction {
                        name: tool.name.clone(),
                        arguments: tool_call_arguments(&tool.input),
                    },
                });
            }
            claude::messages::BetaContentBlock::ServerToolUse(tool) => {
                calls.push(openai::chat_completions::ChatCompletionMessageToolCall::Function {
                    id: tool.id.clone(),
                    function: openai::chat_completions::ChatCompletionToolCallFunction {
                        name: server_tool_name(&tool.name).to_string(),
                        arguments: tool_call_arguments(&tool.input),
                    },
                });
            }
            claude::messages::BetaContentBlock::MCPToolUse(tool) => {
                let name = format!("{}/{}", tool.server_name, tool.name);
                calls.push(openai::chat_completions::ChatCompletionMessageToolCall::Function {
                    id: tool.id.clone(),
                    function: openai::chat_completions::ChatCompletionToolCallFunction {
                        name,
                        arguments: tool_call_arguments(&tool.input),
                    },
                });
            }
            _ => {}
        }
    }
    calls
}

fn usage_from_claude(
    usage: &claude::messages::BetaUsage,
) -> openai::chat_completions::CompletionUsage {
    let prompt_tokens = usage.input_tokens as i64;
    let completion_tokens = usage.output_tokens as i64;
    openai::chat_completions::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        completion_tokens_details: None,
        prompt_tokens_details: None,
        extra_properties: None,
    }
}

pub fn response(
    resp: ResponseParts<claude::messages::MessageCreateResponse>,
) -> Result<ResponseParts<openai::chat_completions::CreateChatCompletionResponse>, TransformError> {
    let text = text_from_blocks(&resp.body.content);
    let tool_calls = tool_calls_from_blocks(&resp.body.content);
    let message = openai::chat_completions::ChatCompletionResponseMessage {
        content: if text.is_empty() { None } else { Some(text) },
        refusal: None,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        annotations: None,
        role: openai::chat_completions::ChatCompletionResponseRole::Assistant,
        function_call: None,
        audio: None,
    };
    let choice = openai::chat_completions::ChatCompletionChoice {
        finish_reason: finish_reason_from_claude(resp.body.stop_reason.clone()),
        index: 0,
        message,
        logprobs: None,
    };
    let body = openai::chat_completions::CreateChatCompletionResponse {
        id: resp.body.id.clone(),
        choices: vec![choice],
        created: OffsetDateTime::now_utc().unix_timestamp(),
        model: resp.body.model.as_str().to_string(),
        service_tier: None,
        system_fingerprint: None,
        object_type: openai::chat_completions::ChatCompletionObjectType::ChatCompletion,
        usage: Some(usage_from_claude(&resp.body.usage)),
    };
    map_status_headers(resp.status, &resp.headers, body)
}
