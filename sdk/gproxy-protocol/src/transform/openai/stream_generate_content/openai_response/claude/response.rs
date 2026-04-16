use std::collections::BTreeMap;

use crate::claude::count_tokens::types::BetaServerToolUseName;
use crate::claude::create_message::stream::{BetaRawContentBlockDelta, ClaudeStreamEvent};
use crate::claude::create_message::types::{BetaContentBlock, BetaStopReason};
use crate::claude::types::BetaError;
use crate::openai::count_tokens::types as ot;
use crate::openai::create_response::stream::{ResponseStreamContentPart, ResponseStreamEvent};
use crate::openai::create_response::types as rt;
use crate::transform::claude::utils::claude_model_to_string;
use crate::transform::openai::stream_generate_content::openai_response::utils::{
    next_sequence_number, push_done_event, push_stream_event, response_snapshot,
    response_usage_from_counts,
};
use crate::transform::utils::TransformError;

#[derive(Debug, Clone)]
enum ClaudeBlockState {
    Text {
        item_id: String,
        text: String,
    },
    Thinking {
        item_id: String,
        text: String,
        signature: String,
    },
    RedactedThinking {
        item_id: String,
        encrypted_content: String,
    },
    FunctionToolCall {
        item_id: String,
        name: String,
        arguments: ToolCallPayload,
    },
    CustomToolCall {
        item_id: String,
        name: String,
        input: ToolCallPayload,
    },
    McpCall {
        item_id: String,
        name: String,
        server_label: String,
        arguments: ToolCallPayload,
    },
    Compaction {
        item_id: String,
        encrypted_content: String,
    },
    Ignore,
}

#[derive(Debug, Clone)]
struct ToolCallPayload {
    initial: String,
    streamed: String,
}

impl ToolCallPayload {
    fn new(initial: String) -> Self {
        Self {
            initial,
            streamed: String::new(),
        }
    }

    fn in_progress(&self) -> &str {
        if !self.streamed.is_empty() {
            &self.streamed
        } else if self.initial == "{}" {
            ""
        } else {
            &self.initial
        }
    }

    fn final_value(&self) -> &str {
        if self.streamed.is_empty() {
            &self.initial
        } else {
            &self.streamed
        }
    }

    fn push_delta(&mut self, delta: &str) {
        if !delta.is_empty() {
            self.streamed.push_str(delta);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeToOpenAiResponseStream {
    next_sequence_number: u64,
    started: bool,
    finished: bool,
    response_id: String,
    model: String,
    input_tokens: u64,
    cache_creation_input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    incomplete_reason: Option<rt::ResponseIncompleteReason>,
    output_text: String,
    blocks: BTreeMap<u64, ClaudeBlockState>,
}

impl ClaudeToOpenAiResponseStream {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn usage(&self) -> Option<rt::ResponseUsage> {
        if !self.started {
            return None;
        }

        let input_tokens = self
            .input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cached_input_tokens);

        Some(response_usage_from_counts(
            input_tokens,
            self.cached_input_tokens,
            self.output_tokens,
            0,
        ))
    }

    fn current_response(
        &self,
        status: Option<rt::ResponseStatus>,
        error: Option<rt::ResponseError>,
    ) -> crate::openai::create_response::response::ResponseBody {
        response_snapshot(
            if self.response_id.is_empty() {
                "response"
            } else {
                &self.response_id
            },
            if self.model.is_empty() {
                "claude"
            } else {
                &self.model
            },
            status,
            self.usage(),
            self.incomplete_reason.clone(),
            error,
            Some(self.output_text.clone()),
        )
    }

    fn append_output_text(&mut self, text: &str) {
        if !text.is_empty() {
            self.output_text.push_str(text);
        }
    }

    fn ensure_started(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        if self.started {
            return;
        }

        let created_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::Created {
                response: self.current_response(Some(rt::ResponseStatus::InProgress), None),
                sequence_number: created_sequence,
            },
        );

        let in_progress_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::InProgress {
                response: self.current_response(Some(rt::ResponseStatus::InProgress), None),
                sequence_number: in_progress_sequence,
            },
        );

        self.started = true;
    }

    fn emit_message_part_added(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        item_id: String,
        output_index: u64,
        text: String,
    ) {
        let item = message_item(
            item_id.clone(),
            text.clone(),
            ot::ResponseItemStatus::InProgress,
        );
        let part = ResponseStreamContentPart::OutputText(output_text_part(text));

        let output_item_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item,
                output_index,
                sequence_number: output_item_sequence,
            },
        );

        let part_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartAdded {
                content_index: 0,
                item_id,
                output_index,
                part,
                sequence_number: part_sequence,
            },
        );
    }

    fn emit_reasoning_part_added(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        item_id: String,
        output_index: u64,
        text: String,
        encrypted_content: Option<String>,
    ) {
        let item = reasoning_item(
            item_id.clone(),
            text.clone(),
            encrypted_content,
            ot::ResponseItemStatus::InProgress,
        );
        let part = ResponseStreamContentPart::ReasoningText(reasoning_text_part(text));

        let output_item_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item,
                output_index,
                sequence_number: output_item_sequence,
            },
        );

        let part_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartAdded {
                content_index: 0,
                item_id,
                output_index,
                part,
                sequence_number: part_sequence,
            },
        );
    }

    fn on_content_block_start(
        &mut self,
        index: u64,
        content_block: BetaContentBlock,
        out: &mut Vec<ResponseStreamEvent>,
    ) {
        if self.finished {
            return;
        }

        self.ensure_started(out);

        match content_block {
            BetaContentBlock::Text(block) => {
                let item_id = format!("{}_message_{index}", self.response_id);
                let text = block.text;
                self.emit_message_part_added(out, item_id.clone(), index, text.clone());

                if !text.is_empty() {
                    self.append_output_text(&text);
                    let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::OutputTextDelta {
                            content_index: 0,
                            delta: text.clone(),
                            item_id: item_id.clone(),
                            logprobs: None,
                            output_index: index,
                            sequence_number: delta_sequence,
                            obfuscation: None,
                        },
                    );
                }

                self.blocks
                    .insert(index, ClaudeBlockState::Text { item_id, text });
            }
            BetaContentBlock::Thinking(block) => {
                let item_id = format!("reasoning_{index}");
                let text = block.thinking;
                self.emit_reasoning_part_added(out, item_id.clone(), index, text.clone(), None);

                if !text.is_empty() {
                    let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ReasoningTextDelta {
                            content_index: 0,
                            delta: text.clone(),
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: delta_sequence,
                            obfuscation: None,
                        },
                    );
                }

                self.blocks.insert(
                    index,
                    ClaudeBlockState::Thinking {
                        item_id,
                        text,
                        signature: block.signature,
                    },
                );
            }
            BetaContentBlock::RedactedThinking(block) => {
                let item_id = format!("redacted_reasoning_{index}");
                let encrypted_content = block.data;

                let output_item_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemAdded {
                        item: reasoning_item(
                            item_id.clone(),
                            String::new(),
                            Some(encrypted_content.clone()),
                            ot::ResponseItemStatus::InProgress,
                        ),
                        output_index: index,
                        sequence_number: output_item_sequence,
                    },
                );

                self.blocks.insert(
                    index,
                    ClaudeBlockState::RedactedThinking {
                        item_id,
                        encrypted_content,
                    },
                );
            }
            BetaContentBlock::ToolUse(block) => {
                let item_id = block.id;
                let name = block.name;
                let arguments = ToolCallPayload::new(
                    serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_string()),
                );

                let added_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemAdded {
                        item: function_tool_call_item(
                            item_id.clone(),
                            name.clone(),
                            arguments.in_progress().to_string(),
                            Some(ot::ResponseItemStatus::InProgress),
                        ),
                        output_index: index,
                        sequence_number: added_sequence,
                    },
                );

                if !arguments.in_progress().is_empty() {
                    let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::FunctionCallArgumentsDelta {
                            delta: arguments.in_progress().to_string(),
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: delta_sequence,
                            obfuscation: None,
                        },
                    );
                }

                self.blocks.insert(
                    index,
                    ClaudeBlockState::FunctionToolCall {
                        item_id,
                        name,
                        arguments,
                    },
                );
            }
            BetaContentBlock::ServerToolUse(block) => {
                let item_id = block.id;
                let name = server_tool_name(&block.name).to_string();
                let input = ToolCallPayload::new(
                    serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_string()),
                );

                let added_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemAdded {
                        item: custom_tool_call_item(
                            item_id.clone(),
                            name.clone(),
                            input.in_progress().to_string(),
                        ),
                        output_index: index,
                        sequence_number: added_sequence,
                    },
                );

                if !input.in_progress().is_empty() {
                    let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::CustomToolCallInputDelta {
                            delta: input.in_progress().to_string(),
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: delta_sequence,
                            obfuscation: None,
                        },
                    );
                }

                self.blocks.insert(
                    index,
                    ClaudeBlockState::CustomToolCall {
                        item_id,
                        name,
                        input,
                    },
                );
            }
            BetaContentBlock::McpToolUse(block) => {
                let item_id = block.id;
                let name = block.name;
                let server_label = block.server_name;
                let arguments = ToolCallPayload::new(
                    serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_string()),
                );

                let added_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemAdded {
                        item: mcp_call_item(
                            item_id.clone(),
                            name.clone(),
                            server_label.clone(),
                            arguments.in_progress().to_string(),
                            Some(ot::ResponseToolCallStatus::InProgress),
                            None,
                            None,
                        ),
                        output_index: index,
                        sequence_number: added_sequence,
                    },
                );

                let in_progress_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::McpCallInProgress {
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number: in_progress_sequence,
                    },
                );

                if !arguments.in_progress().is_empty() {
                    let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::McpCallArgumentsDelta {
                            delta: arguments.in_progress().to_string(),
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: delta_sequence,
                            obfuscation: None,
                        },
                    );
                }

                self.blocks.insert(
                    index,
                    ClaudeBlockState::McpCall {
                        item_id,
                        name,
                        server_label,
                        arguments,
                    },
                );
            }
            BetaContentBlock::Compaction(block) => {
                let item_id = format!("compaction_{index}");
                let encrypted_content = block.content.unwrap_or_default();

                let added_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemAdded {
                        item: compaction_item(item_id.clone(), encrypted_content.clone()),
                        output_index: index,
                        sequence_number: added_sequence,
                    },
                );

                self.blocks.insert(
                    index,
                    ClaudeBlockState::Compaction {
                        item_id,
                        encrypted_content,
                    },
                );
            }
            other => {
                if let Ok(text) = serde_json::to_string(&other) {
                    let item_id = format!("{}_message_{index}", self.response_id);
                    self.emit_message_part_added(out, item_id.clone(), index, text.clone());
                    if !text.is_empty() {
                        self.append_output_text(&text);
                        let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
                        push_stream_event(
                            out,
                            ResponseStreamEvent::OutputTextDelta {
                                content_index: 0,
                                delta: text.clone(),
                                item_id: item_id.clone(),
                                logprobs: None,
                                output_index: index,
                                sequence_number: delta_sequence,
                                obfuscation: None,
                            },
                        );
                    }
                    self.blocks
                        .insert(index, ClaudeBlockState::Text { item_id, text });
                } else {
                    self.blocks.insert(index, ClaudeBlockState::Ignore);
                }
            }
        }
    }

    fn on_content_block_delta(
        &mut self,
        index: u64,
        delta: BetaRawContentBlockDelta,
        out: &mut Vec<ResponseStreamEvent>,
    ) {
        if self.finished {
            return;
        }

        let mut text_delta: Option<(String, String)> = None;
        match (self.blocks.get_mut(&index), delta) {
            (
                Some(ClaudeBlockState::Text { item_id, text }),
                BetaRawContentBlockDelta::Text { text: delta_text },
            ) if !delta_text.is_empty() => {
                text.push_str(&delta_text);
                text_delta = Some((item_id.clone(), delta_text));
            }
            (
                Some(ClaudeBlockState::Thinking { item_id, text, .. }),
                BetaRawContentBlockDelta::Thinking { thinking },
            ) if !thinking.is_empty() => {
                text.push_str(&thinking);
                let sequence_number = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::ReasoningTextDelta {
                        content_index: 0,
                        delta: thinking,
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number,
                        obfuscation: None,
                    },
                );
            }
            (
                Some(ClaudeBlockState::Thinking { signature, .. }),
                BetaRawContentBlockDelta::Signature { signature: new_sig },
            ) => {
                *signature = new_sig;
            }
            (
                Some(ClaudeBlockState::FunctionToolCall {
                    item_id, arguments, ..
                }),
                BetaRawContentBlockDelta::InputJson { partial_json },
            ) if !partial_json.is_empty() => {
                arguments.push_delta(&partial_json);
                let sequence_number = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::FunctionCallArgumentsDelta {
                        delta: partial_json,
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number,
                        obfuscation: None,
                    },
                );
            }
            (
                Some(ClaudeBlockState::CustomToolCall { item_id, input, .. }),
                BetaRawContentBlockDelta::InputJson { partial_json },
            ) if !partial_json.is_empty() => {
                input.push_delta(&partial_json);
                let sequence_number = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::CustomToolCallInputDelta {
                        delta: partial_json,
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number,
                        obfuscation: None,
                    },
                );
            }
            (
                Some(ClaudeBlockState::McpCall {
                    item_id, arguments, ..
                }),
                BetaRawContentBlockDelta::InputJson { partial_json },
            ) if !partial_json.is_empty() => {
                arguments.push_delta(&partial_json);
                let sequence_number = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::McpCallArgumentsDelta {
                        delta: partial_json,
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number,
                        obfuscation: None,
                    },
                );
            }
            (
                Some(ClaudeBlockState::Compaction {
                    encrypted_content, ..
                }),
                BetaRawContentBlockDelta::Compaction { content },
            ) => {
                *encrypted_content = content.unwrap_or_default();
            }
            _ => {}
        }

        if let Some((item_id, delta_text)) = text_delta {
            self.append_output_text(&delta_text);
            let sequence_number = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::OutputTextDelta {
                    content_index: 0,
                    delta: delta_text,
                    item_id,
                    logprobs: None,
                    output_index: index,
                    sequence_number,
                    obfuscation: None,
                },
            );
        }
    }

    fn on_content_block_stop(&mut self, index: u64, out: &mut Vec<ResponseStreamEvent>) {
        if self.finished {
            return;
        }

        let Some(state) = self.blocks.remove(&index) else {
            return;
        };

        match state {
            ClaudeBlockState::Text { item_id, text } => {
                let done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputTextDone {
                        content_index: 0,
                        item_id: item_id.clone(),
                        logprobs: None,
                        output_index: index,
                        sequence_number: done_sequence,
                        text: text.clone(),
                    },
                );

                let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::ContentPartDone {
                        content_index: 0,
                        item_id: item_id.clone(),
                        output_index: index,
                        part: ResponseStreamContentPart::OutputText(output_text_part(text.clone())),
                        sequence_number: part_done_sequence,
                    },
                );

                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: message_item(item_id, text, ot::ResponseItemStatus::Completed),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::Thinking { item_id, text, .. } => {
                let done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::ReasoningTextDone {
                        content_index: 0,
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number: done_sequence,
                        text: text.clone(),
                    },
                );

                if !text.is_empty() {
                    let summary = summary_text_part(text.clone());

                    let part_added_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ReasoningSummaryPartAdded {
                            item_id: item_id.clone(),
                            output_index: index,
                            part: summary.clone(),
                            sequence_number: part_added_sequence,
                            summary_index: 0,
                        },
                    );

                    let summary_delta_sequence =
                        next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ReasoningSummaryTextDelta {
                            delta: text.clone(),
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: summary_delta_sequence,
                            summary_index: 0,
                            obfuscation: None,
                        },
                    );

                    let summary_done_sequence =
                        next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ReasoningSummaryTextDone {
                            item_id: item_id.clone(),
                            output_index: index,
                            sequence_number: summary_done_sequence,
                            summary_index: 0,
                            text: text.clone(),
                        },
                    );

                    let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ReasoningSummaryPartDone {
                            item_id: item_id.clone(),
                            output_index: index,
                            part: summary,
                            sequence_number: part_done_sequence,
                            summary_index: 0,
                        },
                    );
                }

                let content_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::ContentPartDone {
                        content_index: 0,
                        item_id: item_id.clone(),
                        output_index: index,
                        part: ResponseStreamContentPart::ReasoningText(reasoning_text_part(
                            text.clone(),
                        )),
                        sequence_number: content_done_sequence,
                    },
                );

                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: reasoning_item(
                            item_id,
                            text,
                            None,
                            ot::ResponseItemStatus::Completed,
                        ),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::RedactedThinking {
                item_id,
                encrypted_content,
            } => {
                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: reasoning_item(
                            item_id,
                            String::new(),
                            Some(encrypted_content),
                            ot::ResponseItemStatus::Completed,
                        ),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::FunctionToolCall {
                item_id,
                name,
                arguments,
            } => {
                let arguments = arguments.final_value().to_string();
                let arguments_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::FunctionCallArgumentsDone {
                        arguments: arguments.clone(),
                        item_id: item_id.clone(),
                        name: Some(name.clone()),
                        output_index: index,
                        sequence_number: arguments_done_sequence,
                    },
                );

                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: function_tool_call_item(
                            item_id,
                            name,
                            arguments,
                            Some(ot::ResponseItemStatus::Completed),
                        ),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::CustomToolCall {
                item_id,
                name,
                input,
            } => {
                let input = input.final_value().to_string();
                let input_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::CustomToolCallInputDone {
                        input: input.clone(),
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number: input_done_sequence,
                    },
                );

                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: custom_tool_call_item(item_id, name, input),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::McpCall {
                item_id,
                name,
                server_label,
                arguments,
            } => {
                let arguments = arguments.final_value().to_string();
                let arguments_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::McpCallArgumentsDone {
                        arguments: arguments.clone(),
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number: arguments_done_sequence,
                    },
                );

                let completed_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::McpCallCompleted {
                        item_id: item_id.clone(),
                        output_index: index,
                        sequence_number: completed_sequence,
                    },
                );

                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: mcp_call_item(
                            item_id,
                            name,
                            server_label,
                            arguments,
                            Some(ot::ResponseToolCallStatus::Completed),
                            None,
                            None,
                        ),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::Compaction {
                item_id,
                encrypted_content,
            } => {
                let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::OutputItemDone {
                        item: compaction_item(item_id, encrypted_content),
                        output_index: index,
                        sequence_number: output_done_sequence,
                    },
                );
            }
            ClaudeBlockState::Ignore => {}
        }
    }

    fn close_all_open_blocks(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        let indexes = self.blocks.keys().copied().collect::<Vec<_>>();
        for index in indexes {
            self.on_content_block_stop(index, out);
        }
    }

    fn finalize_into(
        &mut self,
        status: rt::ResponseStatus,
        error: Option<rt::ResponseError>,
        out: &mut Vec<ResponseStreamEvent>,
    ) {
        if self.finished {
            return;
        }

        self.ensure_started(out);
        self.close_all_open_blocks(out);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        match status {
            rt::ResponseStatus::Incomplete => {
                push_stream_event(
                    out,
                    ResponseStreamEvent::Incomplete {
                        response: self
                            .current_response(Some(rt::ResponseStatus::Incomplete), error),
                        sequence_number,
                    },
                );
            }
            rt::ResponseStatus::Failed => {
                push_stream_event(
                    out,
                    ResponseStreamEvent::Failed {
                        response: self.current_response(Some(rt::ResponseStatus::Failed), error),
                        sequence_number,
                    },
                );
            }
            _ => {
                push_stream_event(
                    out,
                    ResponseStreamEvent::Completed {
                        response: self.current_response(Some(rt::ResponseStatus::Completed), error),
                        sequence_number,
                    },
                );
            }
        }

        push_done_event(out);
        self.finished = true;
    }

    pub fn finish(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        let status = if self.incomplete_reason.is_some() {
            rt::ResponseStatus::Incomplete
        } else {
            rt::ResponseStatus::Completed
        };
        self.finalize_into(status, None, out);
    }

    pub fn on_event(
        &mut self,
        event: ClaudeStreamEvent,
        out: &mut Vec<ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        if self.finished {
            return Ok(());
        }

        match event {
            ClaudeStreamEvent::MessageStart { message } => {
                self.response_id = message.id;
                self.model = claude_model_to_string(&message.model);
                self.input_tokens = message.usage.input_tokens;
                self.cache_creation_input_tokens = message.usage.cache_creation_input_tokens;
                self.cached_input_tokens = message.usage.cache_read_input_tokens;
                self.output_tokens = message.usage.output_tokens;
                self.incomplete_reason = stop_reason_to_incomplete_reason(message.stop_reason);

                self.ensure_started(out);
            }
            ClaudeStreamEvent::ContentBlockStart {
                content_block,
                index,
            } => {
                self.on_content_block_start(index, content_block, out);
            }
            ClaudeStreamEvent::ContentBlockDelta { delta, index } => {
                self.on_content_block_delta(index, delta, out);
            }
            ClaudeStreamEvent::ContentBlockStop { index } => {
                self.on_content_block_stop(index, out);
            }
            ClaudeStreamEvent::MessageDelta {
                delta,
                usage,
                context_management: _,
            } => {
                if let Some(input_tokens) = usage.input_tokens {
                    self.input_tokens = input_tokens;
                }
                if let Some(cache_creation_input_tokens) = usage.cache_creation_input_tokens {
                    self.cache_creation_input_tokens = cache_creation_input_tokens;
                }
                if let Some(cached_input_tokens) = usage.cache_read_input_tokens {
                    self.cached_input_tokens = cached_input_tokens;
                }
                self.output_tokens = usage.output_tokens;
                if delta.stop_reason.is_some() {
                    self.incomplete_reason = stop_reason_to_incomplete_reason(delta.stop_reason);
                }

                self.ensure_started(out);

                let sequence_number = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::InProgress {
                        response: self.current_response(Some(rt::ResponseStatus::InProgress), None),
                        sequence_number,
                    },
                );
            }
            ClaudeStreamEvent::MessageStop {} => {
                self.finish(out);
            }
            ClaudeStreamEvent::Error { error } => {
                let (code, message) = beta_error_message_and_code(error);
                self.ensure_started(out);

                let error_sequence = next_sequence_number(&mut self.next_sequence_number);
                push_stream_event(
                    out,
                    ResponseStreamEvent::Error {
                        error: crate::openai::create_response::stream::ResponseStreamErrorPayload {
                            type_: "stream_error".to_string(),
                            code: Some(code.clone()),
                            message: message.clone(),
                            param: None,
                        },
                        sequence_number: error_sequence,
                    },
                );

                let response_error = rt::ResponseError {
                    code: response_error_code_from_stream_error_code(&code),
                    message,
                };
                self.finalize_into(rt::ResponseStatus::Failed, Some(response_error), out);
            }
            ClaudeStreamEvent::Ping {} => {}
        }

        Ok(())
    }
}

fn output_text_part(text: String) -> ot::ResponseOutputText {
    ot::ResponseOutputText {
        annotations: Vec::new(),
        logprobs: None,
        text,
        type_: ot::ResponseOutputTextType::OutputText,
    }
}

fn reasoning_text_part(text: String) -> ot::ResponseReasoningTextContent {
    ot::ResponseReasoningTextContent {
        text,
        type_: ot::ResponseReasoningTextContentType::ReasoningText,
    }
}

fn summary_text_part(text: String) -> ot::ResponseSummaryTextContent {
    ot::ResponseSummaryTextContent {
        text,
        type_: ot::ResponseSummaryTextContentType::SummaryText,
    }
}

fn message_item(
    item_id: String,
    text: String,
    status: ot::ResponseItemStatus,
) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
        id: item_id,
        content: vec![ot::ResponseOutputContent::Text(output_text_part(text))],
        role: ot::ResponseOutputMessageRole::Assistant,
        phase: Some(ot::ResponseMessagePhase::FinalAnswer),
        status,
        type_: ot::ResponseOutputMessageType::Message,
    })
}

fn reasoning_item(
    item_id: String,
    text: String,
    encrypted_content: Option<String>,
    status: ot::ResponseItemStatus,
) -> rt::ResponseOutputItem {
    let summary = if text.is_empty() {
        Vec::new()
    } else {
        vec![summary_text_part(text.clone())]
    };
    let content = if text.is_empty() {
        None
    } else {
        Some(vec![reasoning_text_part(text)])
    };

    rt::ResponseOutputItem::ReasoningItem(ot::ResponseReasoningItem {
        id: Some(item_id),
        summary,
        type_: ot::ResponseReasoningItemType::Reasoning,
        content,
        encrypted_content,
        status: Some(status),
    })
}

fn function_tool_call_item(
    item_id: String,
    name: String,
    arguments: String,
    status: Option<ot::ResponseItemStatus>,
) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::FunctionToolCall(ot::ResponseFunctionToolCall {
        arguments,
        call_id: item_id.clone(),
        name,
        type_: ot::ResponseFunctionToolCallType::FunctionCall,
        id: Some(item_id),
        status,
    })
}

fn custom_tool_call_item(item_id: String, name: String, input: String) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::CustomToolCall(ot::ResponseCustomToolCall {
        call_id: item_id.clone(),
        input,
        name,
        type_: ot::ResponseCustomToolCallType::CustomToolCall,
        id: Some(item_id),
    })
}

fn mcp_call_item(
    item_id: String,
    name: String,
    server_label: String,
    arguments: String,
    status: Option<ot::ResponseToolCallStatus>,
    output: Option<String>,
    error: Option<String>,
) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::McpCall(ot::ResponseMcpCall {
        id: item_id,
        arguments,
        name,
        server_label,
        type_: ot::ResponseMcpCallType::McpCall,
        approval_request_id: None,
        error,
        output,
        status,
    })
}

fn compaction_item(item_id: String, encrypted_content: String) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::CompactionItem(ot::ResponseCompactionItemParam {
        encrypted_content,
        type_: ot::ResponseCompactionItemType::Compaction,
        id: Some(item_id),
        created_by: None,
    })
}

fn server_tool_name(name: &BetaServerToolUseName) -> &'static str {
    match name {
        BetaServerToolUseName::WebSearch => "web_search",
        BetaServerToolUseName::WebFetch => "web_fetch",
        BetaServerToolUseName::CodeExecution => "code_execution",
        BetaServerToolUseName::BashCodeExecution => "bash_code_execution",
        BetaServerToolUseName::TextEditorCodeExecution => "text_editor_code_execution",
        BetaServerToolUseName::ToolSearchToolRegex => "tool_search_tool_regex",
        BetaServerToolUseName::ToolSearchToolBm25 => "tool_search_tool_bm25",
    }
}

fn stop_reason_to_incomplete_reason(
    stop_reason: Option<BetaStopReason>,
) -> Option<rt::ResponseIncompleteReason> {
    match stop_reason {
        Some(BetaStopReason::MaxTokens) | Some(BetaStopReason::ModelContextWindowExceeded) => {
            Some(rt::ResponseIncompleteReason::MaxOutputTokens)
        }
        Some(BetaStopReason::Refusal) => Some(rt::ResponseIncompleteReason::ContentFilter),
        _ => None,
    }
}

fn beta_error_message_and_code(error: BetaError) -> (String, String) {
    match error {
        BetaError::InvalidRequest(error) => (error.message, "invalid_request_error".to_string()),
        BetaError::Authentication(error) => (error.message, "authentication_error".to_string()),
        BetaError::Billing(error) => (error.message, "billing_error".to_string()),
        BetaError::Permission(error) => (error.message, "permission_error".to_string()),
        BetaError::NotFound(error) => (error.message, "not_found_error".to_string()),
        BetaError::RateLimit(error) => (error.message, "rate_limit_error".to_string()),
        BetaError::GatewayTimeout(error) => (error.message, "timeout_error".to_string()),
        BetaError::Api(error) => (error.message, "api_error".to_string()),
        BetaError::Overloaded(error) => (error.message, "overloaded_error".to_string()),
    }
}

fn response_error_code_from_stream_error_code(code: &str) -> rt::ResponseErrorCode {
    match code {
        "invalid_request_error" => rt::ResponseErrorCode::InvalidPrompt,
        "rate_limit_error" => rt::ResponseErrorCode::RateLimitExceeded,
        _ => rt::ResponseErrorCode::ServerError,
    }
}
