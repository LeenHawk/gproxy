use std::collections::HashMap;

use time::OffsetDateTime;

use crate::formats::{claude, openai};

use super::TransformError;

#[derive(Debug, Clone)]
struct ToolCallState {
    index: i64,
    id: String,
    name: String,
}

#[derive(Debug, Default)]
pub struct StreamState {
    id: Option<String>,
    model: Option<String>,
    created: i64,
    tool_calls: HashMap<usize, ToolCallState>,
    next_tool_index: i64,
}

impl StreamState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map_event(
        &mut self,
        event: claude::stream::ClaudeStreamEvent,
    ) -> Result<Option<openai::chat_completions::CreateChatCompletionStreamResponse>, TransformError>
    {
        use claude::messages::BetaContentBlock;
        use claude::stream::{ClaudeContentBlockDelta, ClaudeStreamEvent};
        match event {
            ClaudeStreamEvent::MessageStart { message } => {
                if self.created == 0 {
                    self.created = OffsetDateTime::now_utc().unix_timestamp();
                }
                self.id = Some(message.id);
                self.model = Some(message.model.as_str().to_string());
                let delta = openai::chat_completions::ChatCompletionStreamResponseDelta {
                    role: Some(openai::chat_completions::ChatCompletionStreamRole::Assistant),
                    content: None,
                    function_call: None,
                    tool_calls: None,
                    refusal: None,
                    obfuscation: None,
                };
                Ok(Some(self.build_chunk(delta, None, None)?))
            }
            ClaudeStreamEvent::ContentBlockStart { index, content_block } => {
                match content_block {
                    BetaContentBlock::ToolUse(tool) => {
                        let call = ToolCallState {
                            index: self.next_tool_index,
                            id: tool.id,
                            name: tool.name,
                        };
                        self.next_tool_index += 1;
                        self.tool_calls.insert(index, call.clone());
                        Ok(Some(self.build_chunk(
                            tool_call_delta(&call, Some(call.name.clone()), None),
                            None,
                            None,
                        )?))
                    }
                    BetaContentBlock::ServerToolUse(tool) => {
                        let call = ToolCallState {
                            index: self.next_tool_index,
                            id: tool.id,
                            name: server_tool_name(&tool.name).to_string(),
                        };
                        self.next_tool_index += 1;
                        self.tool_calls.insert(index, call.clone());
                        Ok(Some(self.build_chunk(
                            tool_call_delta(&call, Some(call.name.clone()), None),
                            None,
                            None,
                        )?))
                    }
                    BetaContentBlock::MCPToolUse(tool) => {
                        let call = ToolCallState {
                            index: self.next_tool_index,
                            id: tool.id,
                            name: format!("{}/{}", tool.server_name, tool.name),
                        };
                        self.next_tool_index += 1;
                        self.tool_calls.insert(index, call.clone());
                        Ok(Some(self.build_chunk(
                            tool_call_delta(&call, Some(call.name.clone()), None),
                            None,
                            None,
                        )?))
                    }
                    _ => {
                        if let Some(text) = tool_result_text(&content_block) {
                            let delta = openai::chat_completions::ChatCompletionStreamResponseDelta {
                                role: None,
                                content: Some(text),
                                function_call: None,
                                tool_calls: None,
                                refusal: None,
                                obfuscation: None,
                            };
                            Ok(Some(self.build_chunk(delta, None, None)?))
                        } else {
                            Ok(None)
                        }
                    }
                }
            }
            ClaudeStreamEvent::ContentBlockDelta { index, delta } => match delta {
                ClaudeContentBlockDelta::Text { text } => {
                    let delta = openai::chat_completions::ChatCompletionStreamResponseDelta {
                        role: None,
                        content: Some(text),
                        function_call: None,
                        tool_calls: None,
                        refusal: None,
                        obfuscation: None,
                    };
                    Ok(Some(self.build_chunk(delta, None, None)?))
                }
                ClaudeContentBlockDelta::Thinking { thinking } => {
                    let delta = openai::chat_completions::ChatCompletionStreamResponseDelta {
                        role: None,
                        content: Some(thinking),
                        function_call: None,
                        tool_calls: None,
                        refusal: None,
                        obfuscation: None,
                    };
                    Ok(Some(self.build_chunk(delta, None, None)?))
                }
                ClaudeContentBlockDelta::InputJson { partial_json } => {
                    if let Some(call) = self.tool_calls.get(&index) {
                        Ok(Some(self.build_chunk(
                            tool_call_delta(call, None, Some(partial_json)),
                            None,
                            None,
                        )?))
                    } else {
                        Ok(None)
                    }
                }
                _ => Ok(None),
            },
            ClaudeStreamEvent::MessageDelta { delta, usage, .. } => {
                let finish_reason = delta
                    .stop_reason
                    .map(finish_reason_from_claude);
                let usage = usage.and_then(usage_from_stream);
                if finish_reason.is_some() || usage.is_some() {
                    let delta = openai::chat_completions::ChatCompletionStreamResponseDelta {
                        role: None,
                        content: None,
                        function_call: None,
                        tool_calls: None,
                        refusal: None,
                        obfuscation: None,
                    };
                    Ok(Some(self.build_chunk(delta, finish_reason, usage)?))
                } else {
                    Ok(None)
                }
            }
            ClaudeStreamEvent::MessageStop
            | ClaudeStreamEvent::ContentBlockStop { .. }
            | ClaudeStreamEvent::Ping => Ok(None),
            ClaudeStreamEvent::Error { .. } => Err(TransformError::Invalid("stream error")),
        }
    }

    fn build_chunk(
        &self,
        delta: openai::chat_completions::ChatCompletionStreamResponseDelta,
        finish_reason: Option<openai::chat_completions::ChatCompletionFinishReason>,
        usage: Option<openai::chat_completions::CompletionUsage>,
    ) -> Result<openai::chat_completions::CreateChatCompletionStreamResponse, TransformError> {
        let id = self
            .id
            .clone()
            .ok_or(TransformError::Missing("id"))?;
        let model = self
            .model
            .clone()
            .ok_or(TransformError::Missing("model"))?;
        Ok(openai::chat_completions::CreateChatCompletionStreamResponse {
            id,
            choices: vec![openai::chat_completions::ChatCompletionChunkChoice {
                delta,
                logprobs: None,
                finish_reason,
                index: 0,
            }],
            created: if self.created == 0 {
                OffsetDateTime::now_utc().unix_timestamp()
            } else {
                self.created
            },
            model,
            service_tier: None,
            system_fingerprint: None,
            object_type: openai::chat_completions::ChatCompletionChunkObjectType::ChatCompletionChunk,
            usage,
        })
    }
}

fn finish_reason_from_claude(
    reason: claude::messages::BetaStopReason,
) -> openai::chat_completions::ChatCompletionFinishReason {
    use claude::messages::BetaStopReason;
    use openai::chat_completions::ChatCompletionFinishReason;

    match reason {
        BetaStopReason::EndTurn => ChatCompletionFinishReason::Stop,
        BetaStopReason::MaxTokens => ChatCompletionFinishReason::Length,
        BetaStopReason::StopSequence => ChatCompletionFinishReason::Stop,
        BetaStopReason::ToolUse => ChatCompletionFinishReason::ToolCalls,
        BetaStopReason::Refusal => ChatCompletionFinishReason::ContentFilter,
        BetaStopReason::ModelContextWindowExceeded => ChatCompletionFinishReason::Length,
        BetaStopReason::PauseTurn => ChatCompletionFinishReason::Stop,
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

fn tool_result_text(block: &claude::messages::BetaContentBlock) -> Option<String> {
    match block {
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

fn tool_call_delta(
    call: &ToolCallState,
    name: Option<String>,
    arguments: Option<String>,
) -> openai::chat_completions::ChatCompletionStreamResponseDelta {
    let function = openai::chat_completions::ChatCompletionToolCallFunctionChunk {
        name,
        arguments,
    };
    openai::chat_completions::ChatCompletionStreamResponseDelta {
        role: None,
        content: None,
        function_call: None,
        tool_calls: Some(vec![
            openai::chat_completions::ChatCompletionMessageToolCallChunk {
                index: call.index,
                id: Some(call.id.clone()),
                tool_type: Some(openai::chat_completions::ChatCompletionToolCallType::Function),
                function: Some(function),
            },
        ]),
        refusal: None,
        obfuscation: None,
    }
}

fn usage_from_stream(
    usage: claude::stream::ClaudeStreamUsage,
) -> Option<openai::chat_completions::CompletionUsage> {
    let prompt_tokens = usage.input_tokens? as i64;
    let completion_tokens = usage.output_tokens? as i64;
    Some(openai::chat_completions::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        completion_tokens_details: None,
        prompt_tokens_details: None,
        extra_properties: None,
    })
}
