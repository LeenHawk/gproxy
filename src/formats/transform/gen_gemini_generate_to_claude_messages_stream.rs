use crate::formats::{claude, gemini};
use std::collections::HashMap;
use uuid::Uuid;

use super::TransformError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Text,
    Thinking,
}

pub struct ClaudeStreamState {
    message_id: String,
    started: bool,
    current_block: Option<BlockKind>,
    current_block_index: isize,
    thinking_signature_sent: Option<String>,
    has_tool_use: bool,
    input_tokens: u32,
    output_tokens: u32,
    finished: bool,
}

impl Default for ClaudeStreamState {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeStreamState {
    pub fn new() -> Self {
        Self {
            message_id: format!("msg_{}", Uuid::new_v4().simple()),
            started: false,
            current_block: None,
            current_block_index: -1,
            thinking_signature_sent: None,
            has_tool_use: false,
            input_tokens: 0,
            output_tokens: 0,
            finished: false,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &gemini::generate_content::GenerateContentResponse,
        model: &str,
    ) -> Result<Vec<claude::stream::ClaudeStreamEvent>, TransformError> {
        let mut output = Vec::new();
        if !self.started {
            self.started = true;
            output.push(claude::stream::ClaudeStreamEvent::MessageStart {
                message: claude::stream::ClaudeStreamMessage {
                    id: self.message_id.clone(),
                    message_type: claude::messages::MessageObjectType::Message,
                    role: claude::messages::AssistantRole::Assistant,
                    content: Vec::new(),
                    model: claude::types::Model::Other(model.to_string()),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: Some(claude::stream::ClaudeStreamUsage {
                        cache_creation: None,
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        input_tokens: Some(self.input_tokens),
                        output_tokens: Some(self.output_tokens),
                        server_tool_use: None,
                        service_tier: None,
                    }),
                    context_management: None,
                },
            });
        }

        if let Some(usage) = &event.usage_metadata {
            self.input_tokens = usage.prompt_token_count.unwrap_or(0).max(0) as u32;
            self.output_tokens = usage.candidates_token_count.unwrap_or(0).max(0) as u32;
        }

        for candidate in &event.candidates {
            let parts = candidate
                .content
                .as_ref()
                .map(|content| content.parts.as_slice())
                .unwrap_or(&[]);

            for part in parts {
                if part.thought == Some(true) {
                    let thinking_text = match &part.data {
                        gemini::types::PartData::Text { text } => text.as_str(),
                        _ => "",
                    };
                    self.start_block(BlockKind::Thinking, &mut output);
                    output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                        index: self.current_block_index as usize,
                        delta: claude::stream::ClaudeContentBlockDelta::Thinking {
                            thinking: thinking_text.to_string(),
                        },
                    });
                    if let Some(signature) = &part.thought_signature
                        && self.thinking_signature_sent.as_ref() != Some(signature) {
                            output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                                index: self.current_block_index as usize,
                                delta: claude::stream::ClaudeContentBlockDelta::Signature {
                                    signature: signature.clone(),
                                },
                            });
                            self.thinking_signature_sent = Some(signature.clone());
                        }
                    continue;
                }

                match &part.data {
                    gemini::types::PartData::Text { text } => {
                        self.start_block(BlockKind::Text, &mut output);
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: self.current_block_index as usize,
                            delta: claude::stream::ClaudeContentBlockDelta::Text {
                                text: text.clone(),
                            },
                        });
                    }
                    gemini::types::PartData::InlineData { inline_data } => {
                        self.start_block(BlockKind::Text, &mut output);
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: self.current_block_index as usize,
                            delta: claude::stream::ClaudeContentBlockDelta::Text {
                                text: format!(
                                    "![gemini-generated-content](data:{};base64,{})",
                                    inline_data.mime_type, inline_data.data
                                ),
                            },
                        });
                    }
                    gemini::types::PartData::ExecutableCode { executable_code } => {
                        self.start_block(BlockKind::Text, &mut output);
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: self.current_block_index as usize,
                            delta: claude::stream::ClaudeContentBlockDelta::Text {
                                text: format!(
                                    "\n```{}\n{}\n```\n",
                                    match executable_code.language {
                                        gemini::types::Language::Python => "python",
                                        _ => "text",
                                    },
                                    executable_code.code
                                ),
                            },
                        });
                    }
                    gemini::types::PartData::CodeExecutionResult { code_execution_result } => {
                        if let Some(output_text) = &code_execution_result.output {
                            self.start_block(BlockKind::Text, &mut output);
                            let label = match code_execution_result.outcome {
                                gemini::types::Outcome::OutcomeOk => "output",
                                _ => "error",
                            };
                            output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                                index: self.current_block_index as usize,
                                delta: claude::stream::ClaudeContentBlockDelta::Text {
                                    text: format!("\n```{label}\n{output_text}\n```\n"),
                                },
                            });
                        }
                    }
                    gemini::types::PartData::FunctionCall { function_call } => {
                        self.has_tool_use = true;
                        self.close_block(&mut output);
                        let tool_id = function_call
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("toolu_{}", Uuid::new_v4().simple()));
                        let input = function_call
                            .args
                            .clone()
                            .unwrap_or_default()
                            .into_iter()
                            .collect::<HashMap<_, _>>();
                        let input_json = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());

                        self.current_block_index += 1;
                        let block_index = self.current_block_index as usize;
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockStart {
                            index: block_index,
                            content_block: claude::messages::BetaContentBlock::ToolUse(
                                claude::messages::BetaToolUseBlock {
                                    id: tool_id,
                                    input: HashMap::new(),
                                    name: function_call.name.clone(),
                                    block_type: claude::types::ToolUseBlockType::ToolUse,
                                    caller: None,
                                },
                            ),
                        });
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: block_index,
                            delta: claude::stream::ClaudeContentBlockDelta::InputJson {
                                partial_json: input_json,
                            },
                        });
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockStop {
                            index: block_index,
                        });
                        self.current_block = None;
                    }
                    gemini::types::PartData::FunctionResponse { function_response } => {
                        self.start_block(BlockKind::Text, &mut output);
                        let serialized = serde_json::to_string(&function_response.response)
                            .unwrap_or_else(|_| "{}".to_string());
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: self.current_block_index as usize,
                            delta: claude::stream::ClaudeContentBlockDelta::Text {
                                text: serialized,
                            },
                        });
                    }
                    gemini::types::PartData::FileData { file_data } => {
                        self.start_block(BlockKind::Text, &mut output);
                        output.push(claude::stream::ClaudeStreamEvent::ContentBlockDelta {
                            index: self.current_block_index as usize,
                            delta: claude::stream::ClaudeContentBlockDelta::Text {
                                text: format!("file: {}", file_data.file_uri),
                            },
                        });
                    }
                }
            }

            if let Some(reason) = candidate.finish_reason.as_ref() {
                self.finish_reason(reason, &mut output);
            }
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<claude::stream::ClaudeStreamEvent>, TransformError> {
        let mut output = Vec::new();
        if !self.finished {
            self.close_block(&mut output);
            let stop_reason = if self.has_tool_use {
                claude::messages::BetaStopReason::ToolUse
            } else {
                claude::messages::BetaStopReason::EndTurn
            };
            output.push(claude::stream::ClaudeStreamEvent::MessageDelta {
                delta: claude::stream::ClaudeMessageDelta {
                    stop_reason: Some(stop_reason),
                    stop_sequence: None,
                },
                usage: Some(claude::stream::ClaudeStreamUsage {
                    cache_creation: None,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                    input_tokens: Some(self.input_tokens),
                    output_tokens: Some(self.output_tokens),
                    server_tool_use: None,
                    service_tier: None,
                }),
                context_management: None,
            });
            output.push(claude::stream::ClaudeStreamEvent::MessageStop);
            self.finished = true;
        }
        Ok(output)
    }

    fn start_block(
        &mut self,
        kind: BlockKind,
        output: &mut Vec<claude::stream::ClaudeStreamEvent>,
    ) {
        if self.current_block == Some(kind) {
            return;
        }
        self.close_block(output);
        self.current_block_index += 1;
        let index = self.current_block_index as usize;
        let content_block = match kind {
            BlockKind::Text => claude::messages::BetaContentBlock::Text(
                claude::messages::BetaTextBlock {
                    citations: Vec::new(),
                    text: String::new(),
                    block_type: claude::types::TextBlockType::Text,
                },
            ),
            BlockKind::Thinking => claude::messages::BetaContentBlock::Thinking(
                claude::messages::BetaThinkingBlock {
                    signature: None,
                    thinking: String::new(),
                    block_type: claude::types::ThinkingBlockType::Thinking,
                },
            ),
        };
        output.push(claude::stream::ClaudeStreamEvent::ContentBlockStart {
            index,
            content_block,
        });
        self.current_block = Some(kind);
        if kind == BlockKind::Thinking {
            self.thinking_signature_sent = None;
        }
    }

    fn close_block(&mut self, output: &mut Vec<claude::stream::ClaudeStreamEvent>) {
        if self.current_block.is_some() {
            output.push(claude::stream::ClaudeStreamEvent::ContentBlockStop {
                index: self.current_block_index as usize,
            });
            self.current_block = None;
        }
    }

    fn finish_reason(
        &mut self,
        reason: &gemini::types::FinishReason,
        output: &mut Vec<claude::stream::ClaudeStreamEvent>,
    ) {
        if self.finished {
            return;
        }
        self.close_block(output);
        let stop_reason = match reason {
            gemini::types::FinishReason::MaxTokens => claude::messages::BetaStopReason::MaxTokens,
            gemini::types::FinishReason::Stop if self.has_tool_use => {
                claude::messages::BetaStopReason::ToolUse
            }
            _ => claude::messages::BetaStopReason::EndTurn,
        };

        output.push(claude::stream::ClaudeStreamEvent::MessageDelta {
            delta: claude::stream::ClaudeMessageDelta {
                stop_reason: Some(stop_reason),
                stop_sequence: None,
            },
            usage: Some(claude::stream::ClaudeStreamUsage {
                cache_creation: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                input_tokens: Some(self.input_tokens),
                output_tokens: Some(self.output_tokens),
                server_tool_use: None,
                service_tier: None,
            }),
            context_management: None,
        });
        output.push(claude::stream::ClaudeStreamEvent::MessageStop);
        self.finished = true;
    }
}
