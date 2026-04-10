use std::collections::{BTreeMap, BTreeSet};

use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::stream::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionChunkDeltaToolCall,
};
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::stream::{
    ResponseStreamContentPart, ResponseStreamEvent, ResponseStreamTokenLogprob,
    ResponseStreamTopLogprob,
};
use crate::openai::create_response::types as rt;
use crate::transform::openai::stream_generate_content::openai_response::utils::{
    next_sequence_number, push_done_event, push_stream_event, response_snapshot,
    response_usage_from_counts,
};
use crate::transform::utils::TransformError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessagePartKind {
    Text,
    Refusal,
}

#[derive(Debug, Clone)]
struct MessageState {
    item_id: String,
    output_index: u64,
    text: String,
    refusal: String,
    part_order: Vec<MessagePartKind>,
}

impl MessageState {
    fn new(item_id: String, output_index: u64) -> Self {
        Self {
            item_id,
            output_index,
            text: String::new(),
            refusal: String::new(),
            part_order: Vec::new(),
        }
    }

    fn ensure_part(&mut self, kind: MessagePartKind) -> u64 {
        if let Some(index) = self.part_order.iter().position(|part| *part == kind) {
            index as u64
        } else {
            self.part_order.push(kind);
            (self.part_order.len() - 1) as u64
        }
    }

    fn has_part(&self, kind: MessagePartKind) -> bool {
        self.part_order.contains(&kind)
    }

    fn output_content(&self) -> Vec<ot::ResponseOutputContent> {
        self.part_order
            .iter()
            .map(|part| match part {
                MessagePartKind::Text => ot::ResponseOutputContent::Text(ot::ResponseOutputText {
                    annotations: Vec::new(),
                    logprobs: None,
                    text: self.text.clone(),
                    type_: ot::ResponseOutputTextType::OutputText,
                }),
                MessagePartKind::Refusal => {
                    ot::ResponseOutputContent::Refusal(ot::ResponseOutputRefusal {
                        refusal: self.refusal.clone(),
                        type_: ot::ResponseOutputRefusalType::Refusal,
                    })
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct FunctionCallState {
    item_id: String,
    choice_index: u32,
    output_index: u64,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
struct ReasoningState {
    item_id: String,
    choice_index: u32,
    output_index: u64,
    text: String,
}

#[derive(Debug, Clone)]
struct FunctionCallDeltaInput {
    call_key: String,
    choice_index: u32,
    item_id: String,
    name: Option<String>,
    arguments_delta: Option<String>,
    obfuscation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiChatCompletionsToOpenAiResponseStream {
    next_sequence_number: u64,
    next_output_index: u64,
    started: bool,
    finished: bool,
    response_id: String,
    model: String,
    created_at: u64,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_tokens: u64,
    service_tier: Option<rt::ResponseServiceTier>,
    incomplete_reason: Option<rt::ResponseIncompleteReason>,
    output_text: String,
    message_states: BTreeMap<u32, MessageState>,
    function_states: BTreeMap<String, FunctionCallState>,
    reasoning_states: BTreeMap<u32, ReasoningState>,
}

impl OpenAiChatCompletionsToOpenAiResponseStream {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn next_output_index(&mut self) -> u64 {
        let output_index = self.next_output_index;
        self.next_output_index = self.next_output_index.saturating_add(1);
        output_index
    }

    fn map_service_tier(
        service_tier: Option<ct::ChatCompletionServiceTier>,
    ) -> Option<rt::ResponseServiceTier> {
        service_tier.map(|tier| match tier {
            ct::ChatCompletionServiceTier::Auto => rt::ResponseServiceTier::Auto,
            ct::ChatCompletionServiceTier::Default => rt::ResponseServiceTier::Default,
            ct::ChatCompletionServiceTier::Flex => rt::ResponseServiceTier::Flex,
            ct::ChatCompletionServiceTier::Scale => rt::ResponseServiceTier::Scale,
            ct::ChatCompletionServiceTier::Priority => rt::ResponseServiceTier::Priority,
        })
    }

    fn usage(&self) -> Option<rt::ResponseUsage> {
        if !self.started {
            return None;
        }

        Some(response_usage_from_counts(
            self.input_tokens,
            self.cached_input_tokens,
            self.output_tokens,
            self.reasoning_tokens,
        ))
    }

    fn current_response(
        &self,
        status: Option<rt::ResponseStatus>,
    ) -> crate::openai::create_response::response::ResponseBody {
        let mut response = response_snapshot(
            if self.response_id.is_empty() {
                "response"
            } else {
                &self.response_id
            },
            if self.model.is_empty() {
                "chat.completion"
            } else {
                &self.model
            },
            status,
            self.usage(),
            self.incomplete_reason.clone(),
            None,
            Some(self.output_text.clone()),
        );
        response.created_at = self.created_at;
        response.service_tier = self.service_tier.clone();
        response
    }

    fn ensure_started(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        if self.started {
            return;
        }

        let created_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::Created {
                response: self.current_response(Some(rt::ResponseStatus::InProgress)),
                sequence_number: created_sequence,
            },
        );

        let in_progress_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::InProgress {
                response: self.current_response(Some(rt::ResponseStatus::InProgress)),
                sequence_number: in_progress_sequence,
            },
        );

        self.started = true;
    }

    fn update_from_chunk(&mut self, chunk: &ChatCompletionChunk) {
        self.response_id = chunk.id.clone();
        self.model = chunk.model.clone();
        self.created_at = chunk.created;
        if chunk.service_tier.is_some() {
            self.service_tier = Self::map_service_tier(chunk.service_tier.clone());
        }

        if let Some(usage) = chunk.usage.as_ref() {
            self.input_tokens = usage.prompt_tokens;
            self.cached_input_tokens = usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0);
            self.output_tokens = usage.completion_tokens;
            self.reasoning_tokens = usage
                .completion_tokens_details
                .as_ref()
                .and_then(|details| details.reasoning_tokens)
                .unwrap_or(0);
        }
    }

    fn ensure_message_item(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        if self.message_states.contains_key(&choice_index) {
            return;
        }

        let item_id = format!("{}_message_{}", self.response_id, choice_index);
        let output_index = self.next_output_index();
        self.message_states.insert(
            choice_index,
            MessageState::new(item_id.clone(), output_index),
        );

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
                    id: item_id,
                    content: Vec::new(),
                    role: ot::ResponseOutputMessageRole::Assistant,
                    phase: Some(ot::ResponseMessagePhase::FinalAnswer),
                    status: ot::ResponseItemStatus::InProgress,
                    type_: ot::ResponseOutputMessageType::Message,
                }),
                output_index,
                sequence_number,
            },
        );
    }

    fn ensure_message_part(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        choice_index: u32,
        kind: MessagePartKind,
    ) {
        self.ensure_message_item(out, choice_index);

        let (content_index, item_id, output_index, already_exists) = {
            let state = self
                .message_states
                .get_mut(&choice_index)
                .expect("message state exists");
            let already_exists = state.has_part(kind);
            let content_index = state.ensure_part(kind);
            (
                content_index,
                state.item_id.clone(),
                state.output_index,
                already_exists,
            )
        };

        if already_exists {
            return;
        }

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartAdded {
                content_index,
                item_id,
                output_index,
                part: match kind {
                    MessagePartKind::Text => {
                        ResponseStreamContentPart::OutputText(ot::ResponseOutputText {
                            annotations: Vec::new(),
                            logprobs: None,
                            text: String::new(),
                            type_: ot::ResponseOutputTextType::OutputText,
                        })
                    }
                    MessagePartKind::Refusal => {
                        ResponseStreamContentPart::Refusal(ot::ResponseOutputRefusal {
                            refusal: String::new(),
                            type_: ot::ResponseOutputRefusalType::Refusal,
                        })
                    }
                },
                sequence_number,
            },
        );
    }

    fn append_output_text(&mut self, text: &str) {
        if !text.is_empty() {
            self.output_text.push_str(text);
        }
    }

    fn emit_text_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        choice_index: u32,
        text_delta: String,
        logprobs: Option<Vec<ResponseStreamTokenLogprob>>,
        obfuscation: Option<String>,
    ) {
        if text_delta.is_empty() {
            return;
        }

        self.ensure_message_part(out, choice_index, MessagePartKind::Text);

        let (content_index, item_id, output_index) = {
            let state = self
                .message_states
                .get_mut(&choice_index)
                .expect("message state exists");
            let content_index = state.ensure_part(MessagePartKind::Text);
            state.text.push_str(&text_delta);
            (content_index, state.item_id.clone(), state.output_index)
        };

        self.append_output_text(&text_delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputTextDelta {
                content_index,
                delta: text_delta,
                item_id,
                logprobs,
                output_index,
                sequence_number,
                obfuscation,
            },
        );
    }

    fn emit_refusal_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        choice_index: u32,
        refusal_delta: String,
        obfuscation: Option<String>,
    ) {
        if refusal_delta.is_empty() {
            return;
        }

        self.ensure_message_part(out, choice_index, MessagePartKind::Refusal);

        let (content_index, item_id, output_index) = {
            let state = self
                .message_states
                .get_mut(&choice_index)
                .expect("message state exists");
            let content_index = state.ensure_part(MessagePartKind::Refusal);
            state.refusal.push_str(&refusal_delta);
            (content_index, state.item_id.clone(), state.output_index)
        };

        self.append_output_text(&refusal_delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::RefusalDelta {
                content_index,
                delta: refusal_delta,
                item_id,
                output_index,
                sequence_number,
                obfuscation,
            },
        );
    }

    fn ensure_function_call_item(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        call_key: String,
        choice_index: u32,
        item_id: String,
        name: String,
    ) {
        if self.function_states.contains_key(&call_key) {
            return;
        }

        let normalized_name = if name.is_empty() {
            "function".to_string()
        } else {
            name
        };
        let output_index = self.next_output_index();

        let state = FunctionCallState {
            item_id: item_id.clone(),
            choice_index,
            output_index,
            name: normalized_name.clone(),
            arguments: String::new(),
        };
        self.function_states.insert(call_key, state);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: rt::ResponseOutputItem::FunctionToolCall(ot::ResponseFunctionToolCall {
                    arguments: String::new(),
                    call_id: item_id.clone(),
                    name: normalized_name,
                    type_: ot::ResponseFunctionToolCallType::FunctionCall,
                    id: Some(item_id),
                    status: Some(ot::ResponseItemStatus::InProgress),
                }),
                output_index,
                sequence_number,
            },
        );
    }

    fn ensure_reasoning_item(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        if self.reasoning_states.contains_key(&choice_index) {
            return;
        }

        let item_id = format!("{}_reasoning_{}", self.response_id, choice_index);
        let output_index = self.next_output_index();

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: reasoning_item(
                    item_id.clone(),
                    String::new(),
                    ot::ResponseItemStatus::InProgress,
                ),
                output_index,
                sequence_number,
            },
        );

        let part_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartAdded {
                content_index: 0,
                item_id: item_id.clone(),
                output_index,
                part: ResponseStreamContentPart::ReasoningText(reasoning_text_part(String::new())),
                sequence_number: part_sequence,
            },
        );

        self.reasoning_states.insert(
            choice_index,
            ReasoningState {
                item_id,
                choice_index,
                output_index,
                text: String::new(),
            },
        );
    }

    fn emit_reasoning_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        choice_index: u32,
        delta: String,
        obfuscation: Option<String>,
    ) {
        if delta.is_empty() {
            return;
        }

        self.ensure_reasoning_item(out, choice_index);

        let Some(state) = self.reasoning_states.get_mut(&choice_index) else {
            return;
        };
        state.text.push_str(&delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ReasoningTextDelta {
                content_index: 0,
                delta,
                item_id: state.item_id.clone(),
                output_index: state.output_index,
                sequence_number,
                obfuscation,
            },
        );
    }

    fn emit_function_call_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        input: FunctionCallDeltaInput,
    ) {
        let FunctionCallDeltaInput {
            call_key,
            choice_index,
            item_id,
            name,
            arguments_delta,
            obfuscation,
        } = input;

        self.ensure_function_call_item(
            out,
            call_key.clone(),
            choice_index,
            item_id.clone(),
            name.clone().unwrap_or_else(|| "function".to_string()),
        );

        let Some(state) = self.function_states.get_mut(&call_key) else {
            return;
        };

        if let Some(name) = name
            && !name.is_empty()
        {
            state.name = name;
        }

        let Some(arguments_delta) = arguments_delta else {
            return;
        };
        if arguments_delta.is_empty() {
            return;
        }

        state.arguments.push_str(&arguments_delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::FunctionCallArgumentsDelta {
                delta: arguments_delta,
                item_id,
                output_index: state.output_index,
                sequence_number,
                obfuscation,
            },
        );
    }

    fn close_message(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        let Some(state) = self.message_states.remove(&choice_index) else {
            return;
        };

        for (position, part) in state.part_order.iter().enumerate() {
            let content_index = position as u64;
            match part {
                MessagePartKind::Text => {
                    let done_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::OutputTextDone {
                            content_index,
                            item_id: state.item_id.clone(),
                            logprobs: None,
                            output_index: state.output_index,
                            sequence_number: done_sequence,
                            text: state.text.clone(),
                        },
                    );

                    let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ContentPartDone {
                            content_index,
                            item_id: state.item_id.clone(),
                            output_index: state.output_index,
                            part: ResponseStreamContentPart::OutputText(ot::ResponseOutputText {
                                annotations: Vec::new(),
                                logprobs: None,
                                text: state.text.clone(),
                                type_: ot::ResponseOutputTextType::OutputText,
                            }),
                            sequence_number: part_done_sequence,
                        },
                    );
                }
                MessagePartKind::Refusal => {
                    let done_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::RefusalDone {
                            content_index,
                            item_id: state.item_id.clone(),
                            output_index: state.output_index,
                            refusal: state.refusal.clone(),
                            sequence_number: done_sequence,
                        },
                    );

                    let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
                    push_stream_event(
                        out,
                        ResponseStreamEvent::ContentPartDone {
                            content_index,
                            item_id: state.item_id.clone(),
                            output_index: state.output_index,
                            part: ResponseStreamContentPart::Refusal(ot::ResponseOutputRefusal {
                                refusal: state.refusal.clone(),
                                type_: ot::ResponseOutputRefusalType::Refusal,
                            }),
                            sequence_number: part_done_sequence,
                        },
                    );
                }
            }
        }

        let item_id = state.item_id.clone();
        let output_index = state.output_index;
        let content = state.output_content();
        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemDone {
                item: rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
                    id: item_id,
                    content,
                    role: ot::ResponseOutputMessageRole::Assistant,
                    phase: Some(ot::ResponseMessagePhase::FinalAnswer),
                    status: ot::ResponseItemStatus::Completed,
                    type_: ot::ResponseOutputMessageType::Message,
                }),
                output_index,
                sequence_number,
            },
        );
    }

    fn close_reasoning(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        let Some(state) = self.reasoning_states.remove(&choice_index) else {
            return;
        };

        let reasoning_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ReasoningTextDone {
                content_index: 0,
                item_id: state.item_id.clone(),
                output_index: state.output_index,
                sequence_number: reasoning_done_sequence,
                text: state.text.clone(),
            },
        );

        let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartDone {
                content_index: 0,
                item_id: state.item_id.clone(),
                output_index: state.output_index,
                part: ResponseStreamContentPart::ReasoningText(reasoning_text_part(
                    state.text.clone(),
                )),
                sequence_number: part_done_sequence,
            },
        );

        if !state.text.is_empty() {
            let summary = summary_text_part(state.text.clone());

            let summary_added_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ReasoningSummaryPartAdded {
                    item_id: state.item_id.clone(),
                    output_index: state.output_index,
                    part: summary.clone(),
                    sequence_number: summary_added_sequence,
                    summary_index: 0,
                },
            );

            let summary_delta_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ReasoningSummaryTextDelta {
                    delta: state.text.clone(),
                    item_id: state.item_id.clone(),
                    output_index: state.output_index,
                    sequence_number: summary_delta_sequence,
                    summary_index: 0,
                    obfuscation: None,
                },
            );

            let summary_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ReasoningSummaryTextDone {
                    item_id: state.item_id.clone(),
                    output_index: state.output_index,
                    sequence_number: summary_done_sequence,
                    summary_index: 0,
                    text: state.text.clone(),
                },
            );

            let summary_part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ReasoningSummaryPartDone {
                    item_id: state.item_id.clone(),
                    output_index: state.output_index,
                    part: summary,
                    sequence_number: summary_part_done_sequence,
                    summary_index: 0,
                },
            );
        }

        let item_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemDone {
                item: reasoning_item(state.item_id, state.text, ot::ResponseItemStatus::Completed),
                output_index: state.output_index,
                sequence_number: item_done_sequence,
            },
        );
    }

    fn close_function_calls(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        let keys = self
            .function_states
            .iter()
            .filter_map(|(key, value)| {
                if value.choice_index == choice_index {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for key in keys {
            let Some(state) = self.function_states.remove(&key) else {
                continue;
            };

            let done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::FunctionCallArgumentsDone {
                    arguments: state.arguments.clone(),
                    item_id: state.item_id.clone(),
                    name: Some(state.name.clone()),
                    output_index: state.output_index,
                    sequence_number: done_sequence,
                },
            );

            let item_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::OutputItemDone {
                    item: rt::ResponseOutputItem::FunctionToolCall(ot::ResponseFunctionToolCall {
                        arguments: state.arguments,
                        call_id: state.item_id.clone(),
                        name: state.name,
                        type_: ot::ResponseFunctionToolCallType::FunctionCall,
                        id: Some(state.item_id),
                        status: Some(ot::ResponseItemStatus::Completed),
                    }),
                    output_index: state.output_index,
                    sequence_number: item_done_sequence,
                },
            );
        }
    }

    fn finish_choice(&mut self, out: &mut Vec<ResponseStreamEvent>, choice_index: u32) {
        self.close_reasoning(out, choice_index);
        self.close_message(out, choice_index);
        self.close_function_calls(out, choice_index);
    }

    fn finish_reason_to_incomplete_reason(
        finish_reason: ct::ChatCompletionFinishReason,
    ) -> Option<rt::ResponseIncompleteReason> {
        match finish_reason {
            ct::ChatCompletionFinishReason::Length => {
                Some(rt::ResponseIncompleteReason::MaxOutputTokens)
            }
            ct::ChatCompletionFinishReason::ContentFilter => {
                Some(rt::ResponseIncompleteReason::ContentFilter)
            }
            ct::ChatCompletionFinishReason::Stop
            | ct::ChatCompletionFinishReason::ToolCalls
            | ct::ChatCompletionFinishReason::FunctionCall => None,
        }
    }

    fn map_logprobs(
        logprobs: Option<&ct::ChatCompletionLogprobs>,
        refusal: bool,
    ) -> Option<Vec<ResponseStreamTokenLogprob>> {
        let source = if refusal {
            logprobs.and_then(|value| value.refusal.as_ref())
        } else {
            logprobs.and_then(|value| value.content.as_ref())
        }?;

        if source.is_empty() {
            return None;
        }

        let mapped = source
            .iter()
            .map(|token| ResponseStreamTokenLogprob {
                token: token.token.clone(),
                logprob: token.logprob,
                top_logprobs: if token.top_logprobs.is_empty() {
                    None
                } else {
                    Some(
                        token
                            .top_logprobs
                            .iter()
                            .map(|top| ResponseStreamTopLogprob {
                                token: Some(top.token.clone()),
                                logprob: Some(top.logprob),
                            })
                            .collect(),
                    )
                },
            })
            .collect::<Vec<_>>();

        Some(mapped)
    }

    fn on_choice(&mut self, out: &mut Vec<ResponseStreamEvent>, choice: ChatCompletionChunkChoice) {
        let choice_index = choice.index;
        let ChatCompletionChunkChoice {
            delta,
            finish_reason,
            index: _,
            logprobs,
        } = choice;

        if let Some(text_delta) = delta.content {
            self.emit_text_delta(
                out,
                choice_index,
                text_delta,
                Self::map_logprobs(logprobs.as_ref(), false),
                delta.obfuscation.clone(),
            );
        }

        if let Some(reasoning_delta) = delta.reasoning_content {
            self.emit_reasoning_delta(
                out,
                choice_index,
                reasoning_delta,
                delta.obfuscation.clone(),
            );
        }

        if let Some(refusal_delta) = delta.refusal {
            self.emit_refusal_delta(out, choice_index, refusal_delta, delta.obfuscation.clone());
        }

        if let Some(function_call) = delta.function_call {
            self.emit_function_call_delta(
                out,
                FunctionCallDeltaInput {
                    call_key: format!("legacy:{choice_index}"),
                    choice_index,
                    item_id: format!("function_call_{choice_index}"),
                    name: function_call.name,
                    arguments_delta: function_call.arguments,
                    obfuscation: delta.obfuscation.clone(),
                },
            );
        }

        if let Some(tool_calls) = delta.tool_calls {
            for tool_call in tool_calls {
                self.on_tool_call(out, choice_index, tool_call, delta.obfuscation.clone());
            }
        }

        if let Some(finish_reason) = finish_reason {
            if let Some(incomplete_reason) = Self::finish_reason_to_incomplete_reason(finish_reason)
            {
                self.incomplete_reason = Some(incomplete_reason);
            }
            self.finish_choice(out, choice_index);
        }
    }

    fn on_tool_call(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        choice_index: u32,
        tool_call: ChatCompletionChunkDeltaToolCall,
        obfuscation: Option<String>,
    ) {
        let key = format!("tool:{choice_index}:{}", tool_call.index);
        let item_id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| format!("tool_call_{}_{}", choice_index, tool_call.index));

        let function = tool_call.function;
        let name = function.as_ref().and_then(|value| value.name.clone());
        let arguments_delta = function.and_then(|value| value.arguments);

        self.emit_function_call_delta(
            out,
            FunctionCallDeltaInput {
                call_key: key,
                choice_index,
                item_id,
                name,
                arguments_delta,
                obfuscation,
            },
        );
    }

    pub fn on_stream_event(
        &mut self,
        chunk: ChatCompletionChunk,
        out: &mut Vec<ResponseStreamEvent>,
    ) -> Result<(), TransformError> {
        if self.finished {
            return Ok(());
        }

        self.update_from_chunk(&chunk);
        self.ensure_started(out);

        for choice in chunk.choices {
            self.on_choice(out, choice);
        }

        Ok(())
    }

    fn close_all_open_items(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        let choice_indexes = self
            .message_states
            .keys()
            .copied()
            .chain(
                self.reasoning_states
                    .values()
                    .map(|state| state.choice_index),
            )
            .chain(
                self.function_states
                    .values()
                    .map(|state| state.choice_index),
            )
            .collect::<BTreeSet<_>>();

        for choice_index in choice_indexes {
            self.finish_choice(out, choice_index);
        }
    }

    fn finalize(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        if self.finished {
            return;
        }

        self.ensure_started(out);
        self.close_all_open_items(out);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        if self.incomplete_reason.is_some() {
            push_stream_event(
                out,
                ResponseStreamEvent::Incomplete {
                    response: self.current_response(Some(rt::ResponseStatus::Incomplete)),
                    sequence_number,
                },
            );
        } else {
            push_stream_event(
                out,
                ResponseStreamEvent::Completed {
                    response: self.current_response(Some(rt::ResponseStatus::Completed)),
                    sequence_number,
                },
            );
        }

        push_done_event(out);
        self.finished = true;
    }

    pub fn finish(&mut self, out: &mut Vec<ResponseStreamEvent>) -> Result<(), TransformError> {
        self.finalize(out);
        Ok(())
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

fn reasoning_item(
    item_id: String,
    text: String,
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
        encrypted_content: None,
        status: Some(status),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::create_chat_completions::stream::{
        ChatCompletionChunkDelta, ChatCompletionChunkDeltaToolCall,
        ChatCompletionChunkDeltaToolCallType, ChatCompletionFunctionCallDelta,
    };

    #[test]
    fn streaming_chat_reasoning_maps_to_response_reasoning_events() {
        let mut converter = OpenAiChatCompletionsToOpenAiResponseStream::default();
        let mut out = Vec::new();

        converter
            .on_stream_event(
                ChatCompletionChunk {
                    id: "chatcmpl_1".to_string(),
                    choices: vec![ChatCompletionChunkChoice {
                        delta: ChatCompletionChunkDelta {
                            reasoning_content: Some("plan".to_string()),
                            ..ChatCompletionChunkDelta::default()
                        },
                        finish_reason: None,
                        index: 0,
                        logprobs: None,
                    }],
                    created: 1,
                    model: "deepseek-reasoner".to_string(),
                    object:
                        crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
                    service_tier: None,
                    system_fingerprint: None,
                    usage: None,
                },
                &mut out,
            )
            .expect("first chunk");
        converter
            .on_stream_event(
                ChatCompletionChunk {
                    id: "chatcmpl_1".to_string(),
                    choices: vec![ChatCompletionChunkChoice {
                        delta: ChatCompletionChunkDelta::default(),
                        finish_reason: Some(ct::ChatCompletionFinishReason::ToolCalls),
                        index: 0,
                        logprobs: None,
                    }],
                    created: 1,
                    model: "deepseek-reasoner".to_string(),
                    object:
                        crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
                    service_tier: None,
                    system_fingerprint: None,
                    usage: None,
                },
                &mut out,
            )
            .expect("finish chunk");
        converter.finish(&mut out).expect("finish");

        assert!(out.iter().any(|event| {
            matches!(
                event,
                ResponseStreamEvent::ReasoningTextDelta { delta, .. } if delta == "plan"
            )
        }));
        assert!(out.iter().any(|event| {
            matches!(
                event,
                ResponseStreamEvent::OutputItemDone {
                    item: rt::ResponseOutputItem::ReasoningItem(_),
                    ..
                }
            )
        }));
        assert!(
            out.iter()
                .any(|event| { matches!(event, ResponseStreamEvent::Completed { .. }) })
        );
    }

    #[test]
    fn streaming_chat_tool_calls_map_to_response_function_events() {
        let mut converter = OpenAiChatCompletionsToOpenAiResponseStream::default();
        let mut out = Vec::new();

        converter
            .on_stream_event(
                ChatCompletionChunk {
                    id: "chatcmpl_tool".to_string(),
                    choices: vec![ChatCompletionChunkChoice {
                        delta: ChatCompletionChunkDelta {
                            tool_calls: Some(vec![ChatCompletionChunkDeltaToolCall {
                                index: 0,
                                id: Some("call_1".to_string()),
                                function: Some(ChatCompletionFunctionCallDelta {
                                    name: Some("lookup".to_string()),
                                    arguments: Some("{\"q\":".to_string()),
                                }),
                                type_: Some(ChatCompletionChunkDeltaToolCallType::Function),
                            }]),
                            ..ChatCompletionChunkDelta::default()
                        },
                        finish_reason: None,
                        index: 0,
                        logprobs: None,
                    }],
                    created: 2,
                    model: "gpt-5".to_string(),
                    object:
                        crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
                    service_tier: None,
                    system_fingerprint: None,
                    usage: None,
                },
                &mut out,
            )
            .expect("tool chunk 1");
        converter
            .on_stream_event(
                ChatCompletionChunk {
                    id: "chatcmpl_tool".to_string(),
                    choices: vec![ChatCompletionChunkChoice {
                        delta: ChatCompletionChunkDelta {
                            tool_calls: Some(vec![ChatCompletionChunkDeltaToolCall {
                                index: 0,
                                id: Some("call_1".to_string()),
                                function: Some(ChatCompletionFunctionCallDelta {
                                    name: None,
                                    arguments: Some("\"x\"}".to_string()),
                                }),
                                type_: Some(ChatCompletionChunkDeltaToolCallType::Function),
                            }]),
                            ..ChatCompletionChunkDelta::default()
                        },
                        finish_reason: Some(ct::ChatCompletionFinishReason::ToolCalls),
                        index: 0,
                        logprobs: None,
                    }],
                    created: 2,
                    model: "gpt-5".to_string(),
                    object:
                        crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
                    service_tier: None,
                    system_fingerprint: None,
                    usage: Some(ct::CompletionUsage {
                        completion_tokens: 4,
                        prompt_tokens: 8,
                        total_tokens: 12,
                        completion_tokens_details: Some(ct::CompletionTokensDetails {
                            accepted_prediction_tokens: None,
                            audio_tokens: None,
                            reasoning_tokens: Some(0),
                            rejected_prediction_tokens: None,
                        }),
                        prompt_tokens_details: Some(ct::PromptTokensDetails {
                            audio_tokens: None,
                            cached_tokens: Some(2),
                        }),
                    }),
                },
                &mut out,
            )
            .expect("tool chunk 2");
        converter.finish(&mut out).expect("finish");

        assert!(out.iter().any(|event| {
            matches!(
                event,
                ResponseStreamEvent::OutputItemAdded {
                    item: rt::ResponseOutputItem::FunctionToolCall(call),
                    ..
                } if call.name == "lookup"
            )
        }));
        let deltas = out
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ResponseStreamEvent::FunctionCallArgumentsDelta { .. }
                )
            })
            .count();
        assert_eq!(deltas, 2);
        assert!(out.iter().any(|event| {
            matches!(
                event,
                ResponseStreamEvent::FunctionCallArgumentsDone { arguments, .. }
                if arguments == "{\"q\":\"x\"}"
            )
        }));
        assert!(out.iter().any(|event| {
            matches!(
                event,
                ResponseStreamEvent::Completed { response, .. }
                if response.usage.as_ref().map(|usage| usage.total_tokens) == Some(12)
            )
        }));
    }
}
