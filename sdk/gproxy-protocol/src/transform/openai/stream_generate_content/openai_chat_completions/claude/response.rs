use std::collections::{BTreeMap, BTreeSet};

use crate::claude::count_tokens::types::BetaServerToolUseName;
use crate::claude::create_message::stream::{BetaRawContentBlockDelta, ClaudeStreamEvent};
use crate::claude::create_message::types::{BetaContentBlock, BetaStopReason};
use crate::openai::create_chat_completions::stream::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionChunkDelta,
    ChatCompletionChunkDeltaToolCall, ChatCompletionChunkDeltaToolCallType,
    ChatCompletionFunctionCallDelta,
};
use crate::openai::create_chat_completions::types as ct;
use crate::transform::claude::utils::claude_model_to_string;
use crate::transform::utils::TransformError;

#[derive(Debug, Clone)]
struct OpenAiChatToolState {
    choice_index: u32,
    tool_index: u32,
    call_id: String,
    name: String,
    name_emitted: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ClaudeToOpenAiChatCompletionsStream {
    response_id: String,
    model: String,
    created: u64,
    input_tokens: u64,
    cache_creation_input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    incomplete_finish_reason: Option<ct::ChatCompletionFinishReason>,
    output_choice_map: BTreeMap<u64, u32>,
    role_emitted: BTreeSet<u32>,
    choice_tool_counts: BTreeMap<u32, u32>,
    choice_has_tool_calls: BTreeSet<u32>,
    text_blocks: BTreeSet<u64>,
    thinking_blocks: BTreeSet<u64>,
    tool_blocks: BTreeMap<u64, String>,
    tool_states: BTreeMap<String, OpenAiChatToolState>,
    started: bool,
    finished: bool,
}

impl ClaudeToOpenAiChatCompletionsStream {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn stop_reason_to_finish_reason(
        stop_reason: Option<BetaStopReason>,
    ) -> Option<ct::ChatCompletionFinishReason> {
        match stop_reason {
            Some(BetaStopReason::MaxTokens) | Some(BetaStopReason::ModelContextWindowExceeded) => {
                Some(ct::ChatCompletionFinishReason::Length)
            }
            Some(BetaStopReason::Refusal) => Some(ct::ChatCompletionFinishReason::ContentFilter),
            _ => None,
        }
    }

    fn fallback_response_id(&self) -> String {
        if self.response_id.is_empty() {
            "response".to_string()
        } else {
            self.response_id.clone()
        }
    }

    fn fallback_model(&self) -> String {
        if self.model.is_empty() {
            "claude".to_string()
        } else {
            self.model.clone()
        }
    }

    fn usage(&self) -> Option<ct::CompletionUsage> {
        if !self.started {
            return None;
        }

        let prompt_tokens = self
            .input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cached_input_tokens);

        Some(ct::CompletionUsage {
            completion_tokens: self.output_tokens,
            prompt_tokens,
            total_tokens: prompt_tokens.saturating_add(self.output_tokens),
            completion_tokens_details: Some(ct::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(0),
                rejected_prediction_tokens: None,
            }),
            prompt_tokens_details: Some(ct::PromptTokensDetails {
                audio_tokens: None,
                cached_tokens: Some(self.cached_input_tokens),
            }),
        })
    }

    fn make_chunk(
        &self,
        index: u32,
        delta: ChatCompletionChunkDelta,
        finish_reason: Option<ct::ChatCompletionFinishReason>,
        usage: Option<ct::CompletionUsage>,
    ) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.fallback_response_id(),
            choices: vec![ChatCompletionChunkChoice {
                delta,
                finish_reason,
                index,
                logprobs: None,
            }],
            created: self.created,
            model: self.fallback_model(),
            object: crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
            service_tier: None,
            system_fingerprint: None,
            usage,
        }
    }

    fn ensure_choice_index(&mut self, output_index: u64) -> u32 {
        self.output_choice_map.insert(output_index, 0);
        0
    }

    fn maybe_emit_role(&mut self, out: &mut Vec<ChatCompletionChunk>, choice_index: u32) {
        if self.role_emitted.insert(choice_index) {
            out.push(self.make_chunk(
                choice_index,
                ChatCompletionChunkDelta {
                    role: Some(ct::ChatCompletionDeltaRole::Assistant),
                    ..Default::default()
                },
                None,
                None,
            ));
        }
    }

    fn emit_content(
        &mut self,
        output_index: u64,
        text: String,
        refusal: bool,
        out: &mut Vec<ChatCompletionChunk>,
    ) {
        let choice_index = self.ensure_choice_index(output_index);
        self.maybe_emit_role(out, choice_index);

        if text.is_empty() {
            return;
        }

        out.push(self.make_chunk(
            choice_index,
            ChatCompletionChunkDelta {
                content: if refusal { None } else { Some(text.clone()) },
                refusal: if refusal { Some(text) } else { None },
                ..Default::default()
            },
            None,
            None,
        ));
    }

    fn emit_reasoning_content(
        &mut self,
        output_index: u64,
        text: String,
        out: &mut Vec<ChatCompletionChunk>,
    ) {
        if text.is_empty() {
            return;
        }

        let choice_index = self.ensure_choice_index(output_index);
        self.maybe_emit_role(out, choice_index);

        out.push(self.make_chunk(
            choice_index,
            ChatCompletionChunkDelta {
                reasoning_content: Some(text),
                ..Default::default()
            },
            None,
            None,
        ));
    }

    fn emit_reasoning_signature(
        &mut self,
        output_index: u64,
        signature: String,
        out: &mut Vec<ChatCompletionChunk>,
    ) {
        if signature.is_empty() {
            return;
        }

        let choice_index = self.ensure_choice_index(output_index);
        self.maybe_emit_role(out, choice_index);
        let reasoning_id = format!("reasoning_{output_index}");

        out.push(self.make_chunk(
            choice_index,
            ChatCompletionChunkDelta {
                reasoning_details: Some(vec![ct::ChatCompletionReasoningDetail {
                    type_: ct::ChatCompletionReasoningDetailType::ReasoningEncrypted,
                    id: Some(reasoning_id),
                    data: Some(signature),
                }]),
                ..Default::default()
            },
            None,
            None,
        ));
    }

    fn emit_tool_call_arguments_delta(
        &mut self,
        call_id: &str,
        delta: String,
        out: &mut Vec<ChatCompletionChunk>,
    ) {
        if delta.is_empty() {
            return;
        }

        if let Some(tool) = self.tool_states.get(call_id).cloned() {
            self.maybe_emit_role(out, tool.choice_index);
            out.push(self.make_chunk(
                tool.choice_index,
                ChatCompletionChunkDelta {
                    tool_calls: Some(vec![ChatCompletionChunkDeltaToolCall {
                        index: tool.tool_index,
                        id: Some(tool.call_id.clone()),
                        function: Some(ChatCompletionFunctionCallDelta {
                            name: if tool.name_emitted {
                                None
                            } else {
                                Some(tool.name.clone())
                            },
                            arguments: Some(delta),
                        }),
                        type_: Some(ChatCompletionChunkDeltaToolCallType::Function),
                    }]),
                    ..Default::default()
                },
                None,
                None,
            ));

            if let Some(tool_state) = self.tool_states.get_mut(call_id) {
                tool_state.name_emitted = true;
            }
        }
    }

    fn start_tool_call(
        &mut self,
        output_index: u64,
        call_id: String,
        name: String,
        initial_arguments: String,
        count_for_finish_reason: bool,
        out: &mut Vec<ChatCompletionChunk>,
    ) {
        let choice_index = self.ensure_choice_index(output_index);
        self.maybe_emit_role(out, choice_index);

        let tool_index_ref = self.choice_tool_counts.entry(choice_index).or_insert(0);
        let tool_index = *tool_index_ref;
        *tool_index_ref = tool_index.saturating_add(1);

        if count_for_finish_reason {
            self.choice_has_tool_calls.insert(choice_index);
        }

        let state = OpenAiChatToolState {
            choice_index,
            tool_index,
            call_id: call_id.clone(),
            name,
            name_emitted: false,
        };
        self.tool_blocks.insert(output_index, call_id.clone());
        self.tool_states.insert(call_id.clone(), state.clone());

        out.push(self.make_chunk(
            choice_index,
            ChatCompletionChunkDelta {
                tool_calls: Some(vec![ChatCompletionChunkDeltaToolCall {
                    index: state.tool_index,
                    id: Some(state.call_id.clone()),
                    function: Some(ChatCompletionFunctionCallDelta {
                        name: Some(state.name.clone()),
                        arguments: None,
                    }),
                    type_: Some(ChatCompletionChunkDeltaToolCallType::Function),
                }]),
                ..Default::default()
            },
            None,
            None,
        ));

        if let Some(tool) = self.tool_states.get_mut(&call_id) {
            tool.name_emitted = true;
        }

        if !initial_arguments.is_empty() && initial_arguments != "{}" {
            self.emit_tool_call_arguments_delta(&call_id, initial_arguments, out);
        }
    }

    fn default_finish_reason(&self) -> ct::ChatCompletionFinishReason {
        if let Some(reason) = self.incomplete_finish_reason.clone() {
            return reason;
        }

        if self.choice_has_tool_calls.is_empty() {
            ct::ChatCompletionFinishReason::Stop
        } else {
            ct::ChatCompletionFinishReason::ToolCalls
        }
    }

    fn sorted_choice_indexes(&self) -> Vec<u32> {
        let mut indexes = self.output_choice_map.values().copied().collect::<Vec<_>>();
        indexes.sort_unstable();
        indexes.dedup();
        indexes
    }

    pub fn on_event(
        &mut self,
        event: ClaudeStreamEvent,
        out: &mut Vec<ChatCompletionChunk>,
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
                self.incomplete_finish_reason =
                    Self::stop_reason_to_finish_reason(message.stop_reason);
                self.started = true;
            }
            ClaudeStreamEvent::ContentBlockStart {
                content_block,
                index,
            } => {
                let output_index = index;
                match content_block {
                    BetaContentBlock::Text(block) => {
                        self.text_blocks.insert(output_index);
                        self.emit_content(output_index, block.text, false, out);
                    }
                    BetaContentBlock::Thinking(_) | BetaContentBlock::RedactedThinking(_) => {
                        self.thinking_blocks.insert(output_index);
                    }
                    BetaContentBlock::ToolUse(block) => {
                        let arguments = serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_string());
                        self.start_tool_call(
                            output_index,
                            block.id,
                            block.name,
                            arguments,
                            true,
                            out,
                        );
                    }
                    BetaContentBlock::ServerToolUse(block) => {
                        let arguments = serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_string());
                        self.start_tool_call(
                            output_index,
                            block.id,
                            server_tool_name(&block.name).to_string(),
                            arguments,
                            true,
                            out,
                        );
                    }
                    BetaContentBlock::McpToolUse(block) => {
                        let arguments = serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_string());
                        self.start_tool_call(
                            output_index,
                            block.id,
                            block.name,
                            arguments,
                            true,
                            out,
                        );
                    }
                    other => {
                        if let Ok(text) = serde_json::to_string(&other) {
                            self.text_blocks.insert(output_index);
                            self.emit_content(output_index, text, false, out);
                        }
                    }
                }
            }
            ClaudeStreamEvent::ContentBlockDelta { delta, index } => match delta {
                BetaRawContentBlockDelta::Text { text }
                    if self.text_blocks.contains(&index) =>
                {
                    self.emit_content(index, text, false, out);
                }
                BetaRawContentBlockDelta::Thinking { thinking }
                    if self.thinking_blocks.contains(&index) =>
                {
                    self.emit_reasoning_content(index, thinking, out);
                }
                BetaRawContentBlockDelta::Signature { signature }
                    if self.thinking_blocks.contains(&index) =>
                {
                    self.emit_reasoning_signature(index, signature, out);
                }
                BetaRawContentBlockDelta::InputJson { partial_json } => {
                    if let Some(call_id) = self.tool_blocks.get(&index).cloned() {
                        self.emit_tool_call_arguments_delta(&call_id, partial_json, out);
                    }
                }
                _ => {}
            },
            ClaudeStreamEvent::ContentBlockStop { index } => {
                self.text_blocks.remove(&index);
                self.thinking_blocks.remove(&index);
                if let Some(call_id) = self.tool_blocks.remove(&index) {
                    self.tool_states.remove(&call_id);
                }
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
                    self.incomplete_finish_reason =
                        Self::stop_reason_to_finish_reason(delta.stop_reason);
                }
            }
            ClaudeStreamEvent::MessageStop {} => {
                self.finish(out);
            }
            ClaudeStreamEvent::Error { .. } => {
                self.finished = true;
            }
            ClaudeStreamEvent::Ping {} => {}
        }

        Ok(())
    }

    pub fn finish(&mut self, out: &mut Vec<ChatCompletionChunk>) {
        if self.finished {
            return;
        }

        let default_reason = self.default_finish_reason();

        let mut choices = self.sorted_choice_indexes();
        if choices.is_empty() {
            choices.push(0);
        }

        for choice_index in &choices {
            let finish_reason = if self.choice_has_tool_calls.contains(choice_index) {
                ct::ChatCompletionFinishReason::ToolCalls
            } else {
                default_reason.clone()
            };
            out.push(self.make_chunk(*choice_index, Default::default(), Some(finish_reason), None));
        }

        if let Some(last) = out.last_mut() {
            last.usage = self.usage();
        }

        self.finished = true;
    }
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
