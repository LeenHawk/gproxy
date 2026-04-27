use std::collections::BTreeMap;

use crate::gemini::generate_content::response::ResponseBody as GeminiGenerateContentResponseBody;
use crate::gemini::generate_content::types::{GeminiBlockReason, GeminiFinishReason};
use crate::openai::count_tokens::types as ot;
use crate::openai::create_response::stream::{ResponseStreamContentPart, ResponseStreamEvent};
use crate::openai::create_response::types as rt;
use crate::transform::openai::generate_content::openai_chat_completions::gemini::utils::{
    gemini_function_response_to_text, json_object_to_string, prompt_feedback_refusal_text,
};
use crate::transform::openai::model_list::gemini::utils::strip_models_prefix;
use crate::transform::openai::stream_generate_content::openai_response::utils::{
    next_sequence_number, push_done_event, push_stream_event, response_snapshot,
    response_usage_from_counts,
};

#[derive(Debug, Clone)]
struct MessageState {
    item_id: String,
    text: String,
    refusal: bool,
}

#[derive(Debug, Clone)]
struct ReasoningState {
    item_id: String,
    text: String,
}

#[derive(Debug, Clone)]
struct FunctionCallState {
    item_id: String,
    output_index: u64,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct GeminiToOpenAiResponseStream {
    next_sequence_number: u64,
    chunk_sequence: u64,
    started: bool,
    finished: bool,
    response_id: String,
    model: String,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_tokens: u64,
    incomplete_reason: Option<rt::ResponseIncompleteReason>,
    output_text: String,
    message_items: BTreeMap<u64, MessageState>,
    reasoning_items: BTreeMap<u64, ReasoningState>,
    function_calls: BTreeMap<String, FunctionCallState>,
}

impl GeminiToOpenAiResponseStream {
    pub fn is_finished(&self) -> bool {
        self.finished
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
        error: Option<rt::ResponseError>,
    ) -> crate::openai::create_response::response::ResponseBody {
        response_snapshot(
            if self.response_id.is_empty() {
                "response"
            } else {
                &self.response_id
            },
            if self.model.is_empty() {
                "gemini"
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

    fn update_envelope_from_chunk(&mut self, chunk: &GeminiGenerateContentResponseBody) {
        if let Some(response_id) = chunk.response_id.as_ref() {
            self.response_id = response_id.clone();
        }
        if let Some(model_version) = chunk.model_version.as_ref() {
            self.model = strip_models_prefix(model_version);
        }
        if let Some(usage) = chunk.usage_metadata.as_ref() {
            self.input_tokens = usage
                .prompt_token_count
                .unwrap_or(0)
                .saturating_add(usage.tool_use_prompt_token_count.unwrap_or(0));
            self.cached_input_tokens = usage.cached_content_token_count.unwrap_or(0);
            self.output_tokens = usage
                .candidates_token_count
                .unwrap_or(0)
                .saturating_add(usage.thoughts_token_count.unwrap_or(0));
            self.reasoning_tokens = usage.thoughts_token_count.unwrap_or(0);
        }
    }

    fn ensure_message_item(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
        refusal: bool,
    ) {
        if self.message_items.contains_key(&output_index) {
            if refusal && let Some(state) = self.message_items.get_mut(&output_index) {
                state.refusal = true;
            }
            return;
        }

        let item_id = format!("{}_message_{output_index}", self.response_id);

        let added_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: message_item(
                    item_id.clone(),
                    String::new(),
                    ot::ResponseItemStatus::InProgress,
                    refusal,
                ),
                output_index,
                sequence_number: added_sequence,
            },
        );

        let part_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ContentPartAdded {
                content_index: 0,
                item_id: item_id.clone(),
                output_index,
                part: if refusal {
                    ResponseStreamContentPart::Refusal(refusal_part(String::new()))
                } else {
                    ResponseStreamContentPart::OutputText(output_text_part(String::new()))
                },
                sequence_number: part_sequence,
            },
        );

        self.message_items.insert(
            output_index,
            MessageState {
                item_id,
                text: String::new(),
                refusal,
            },
        );
    }

    fn emit_message_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
        delta: String,
        refusal: bool,
    ) {
        self.ensure_message_item(out, output_index, refusal);
        if delta.is_empty() {
            return;
        }

        let (item_id, is_refusal) = {
            let state = self
                .message_items
                .get_mut(&output_index)
                .expect("message state exists");
            state.refusal = state.refusal || refusal;
            state.text.push_str(&delta);
            (state.item_id.clone(), state.refusal)
        };

        self.append_output_text(&delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        if is_refusal {
            push_stream_event(
                out,
                ResponseStreamEvent::RefusalDelta {
                    content_index: 0,
                    delta,
                    item_id,
                    output_index,
                    sequence_number,
                    obfuscation: None,
                },
            );
        } else {
            push_stream_event(
                out,
                ResponseStreamEvent::OutputTextDelta {
                    content_index: 0,
                    delta,
                    item_id,
                    logprobs: None,
                    output_index,
                    sequence_number,
                    obfuscation: None,
                },
            );
        }
    }

    fn ensure_reasoning_item(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
        item_id: String,
    ) {
        if self.reasoning_items.contains_key(&output_index) {
            return;
        }

        let added_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: reasoning_item(
                    item_id.clone(),
                    String::new(),
                    ot::ResponseItemStatus::InProgress,
                ),
                output_index,
                sequence_number: added_sequence,
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

        self.reasoning_items.insert(
            output_index,
            ReasoningState {
                item_id,
                text: String::new(),
            },
        );
    }

    fn emit_reasoning_delta(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
        item_id: String,
        delta: String,
    ) {
        self.ensure_reasoning_item(out, output_index, item_id);
        if delta.is_empty() {
            return;
        }

        let state = self
            .reasoning_items
            .get_mut(&output_index)
            .expect("reasoning state exists");
        state.text.push_str(&delta);

        let sequence_number = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ReasoningTextDelta {
                content_index: 0,
                delta,
                item_id: state.item_id.clone(),
                output_index,
                sequence_number,
                obfuscation: None,
            },
        );
    }

    fn emit_function_call_snapshot(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
        item_id: String,
        name: String,
        arguments_snapshot: String,
    ) {
        if let Some(state) = self.function_calls.get_mut(&item_id) {
            if !name.is_empty() {
                state.name = name;
            }

            let delta = if arguments_snapshot.starts_with(&state.arguments) {
                arguments_snapshot[state.arguments.len()..].to_string()
            } else {
                arguments_snapshot.clone()
            };

            state.arguments = arguments_snapshot;
            if delta.is_empty() {
                return;
            }

            let sequence_number = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::FunctionCallArgumentsDelta {
                    delta,
                    item_id: state.item_id.clone(),
                    output_index: state.output_index,
                    sequence_number,
                    obfuscation: None,
                },
            );
            return;
        }

        let added_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemAdded {
                item: function_tool_call_item(
                    item_id.clone(),
                    name.clone(),
                    arguments_snapshot.clone(),
                    Some(ot::ResponseItemStatus::InProgress),
                ),
                output_index,
                sequence_number: added_sequence,
            },
        );

        if !arguments_snapshot.is_empty() && arguments_snapshot != "{}" {
            let delta_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::FunctionCallArgumentsDelta {
                    delta: arguments_snapshot.clone(),
                    item_id: item_id.clone(),
                    output_index,
                    sequence_number: delta_sequence,
                    obfuscation: None,
                },
            );
        }

        self.function_calls.insert(
            item_id.clone(),
            FunctionCallState {
                item_id,
                output_index,
                name,
                arguments: arguments_snapshot,
            },
        );
    }

    fn close_message(&mut self, out: &mut Vec<ResponseStreamEvent>, output_index: u64) {
        let Some(state) = self.message_items.remove(&output_index) else {
            return;
        };

        if state.refusal {
            let refusal_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::RefusalDone {
                    content_index: 0,
                    item_id: state.item_id.clone(),
                    output_index,
                    refusal: state.text.clone(),
                    sequence_number: refusal_done_sequence,
                },
            );

            let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ContentPartDone {
                    content_index: 0,
                    item_id: state.item_id.clone(),
                    output_index,
                    part: ResponseStreamContentPart::Refusal(refusal_part(state.text.clone())),
                    sequence_number: part_done_sequence,
                },
            );
        } else {
            let output_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::OutputTextDone {
                    content_index: 0,
                    item_id: state.item_id.clone(),
                    logprobs: None,
                    output_index,
                    sequence_number: output_done_sequence,
                    text: state.text.clone(),
                },
            );

            let part_done_sequence = next_sequence_number(&mut self.next_sequence_number);
            push_stream_event(
                out,
                ResponseStreamEvent::ContentPartDone {
                    content_index: 0,
                    item_id: state.item_id.clone(),
                    output_index,
                    part: ResponseStreamContentPart::OutputText(output_text_part(
                        state.text.clone(),
                    )),
                    sequence_number: part_done_sequence,
                },
            );
        }

        let item_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemDone {
                item: message_item(
                    state.item_id,
                    state.text,
                    ot::ResponseItemStatus::Completed,
                    state.refusal,
                ),
                output_index,
                sequence_number: item_done_sequence,
            },
        );
    }

    fn close_reasoning(&mut self, out: &mut Vec<ResponseStreamEvent>, output_index: u64) {
        let Some(state) = self.reasoning_items.remove(&output_index) else {
            return;
        };

        let reasoning_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::ReasoningTextDone {
                content_index: 0,
                item_id: state.item_id.clone(),
                output_index,
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
                output_index,
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
                    output_index,
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
                    output_index,
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
                    output_index,
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
                    output_index,
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
                output_index,
                sequence_number: item_done_sequence,
            },
        );
    }

    fn close_function_call(&mut self, out: &mut Vec<ResponseStreamEvent>, item_id: String) {
        let Some(state) = self.function_calls.remove(&item_id) else {
            return;
        };

        let arguments_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::FunctionCallArgumentsDone {
                arguments: state.arguments.clone(),
                item_id: state.item_id.clone(),
                name: Some(state.name.clone()),
                output_index: state.output_index,
                sequence_number: arguments_done_sequence,
            },
        );

        let item_done_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::OutputItemDone {
                item: function_tool_call_item(
                    state.item_id,
                    state.name,
                    state.arguments,
                    Some(ot::ResponseItemStatus::Completed),
                ),
                output_index: state.output_index,
                sequence_number: item_done_sequence,
            },
        );
    }

    fn close_function_calls_for_output(
        &mut self,
        out: &mut Vec<ResponseStreamEvent>,
        output_index: u64,
    ) {
        let call_ids = self
            .function_calls
            .iter()
            .filter_map(|(call_id, state)| {
                if state.output_index == output_index {
                    Some(call_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for call_id in call_ids {
            self.close_function_call(out, call_id);
        }
    }

    fn finish_output_index(&mut self, out: &mut Vec<ResponseStreamEvent>, output_index: u64) {
        self.close_message(out, output_index);
        self.close_reasoning(out, output_index);
        self.close_function_calls_for_output(out, output_index);
    }

    fn map_finish_reason(reason: &GeminiFinishReason) -> Option<rt::ResponseIncompleteReason> {
        match reason {
            GeminiFinishReason::MaxTokens => Some(rt::ResponseIncompleteReason::MaxOutputTokens),
            GeminiFinishReason::Safety
            | GeminiFinishReason::Recitation
            | GeminiFinishReason::Blocklist
            | GeminiFinishReason::ProhibitedContent
            | GeminiFinishReason::Spii
            | GeminiFinishReason::ImageSafety
            | GeminiFinishReason::ImageProhibitedContent
            | GeminiFinishReason::ImageRecitation => {
                Some(rt::ResponseIncompleteReason::ContentFilter)
            }
            _ => None,
        }
    }

    fn map_block_reason(reason: &GeminiBlockReason) -> Option<rt::ResponseIncompleteReason> {
        match reason {
            GeminiBlockReason::Safety
            | GeminiBlockReason::Blocklist
            | GeminiBlockReason::ProhibitedContent
            | GeminiBlockReason::ImageSafety => Some(rt::ResponseIncompleteReason::ContentFilter),
            _ => None,
        }
    }

    pub fn on_chunk(
        &mut self,
        chunk: GeminiGenerateContentResponseBody,
        out: &mut Vec<ResponseStreamEvent>,
    ) {
        if self.finished {
            return;
        }

        self.update_envelope_from_chunk(&chunk);
        self.ensure_started(out);

        if let Some(reason) = chunk
            .prompt_feedback
            .as_ref()
            .and_then(|feedback| feedback.block_reason.as_ref())
            .and_then(Self::map_block_reason)
        {
            self.incomplete_reason = Some(reason);
        }

        if let Some(refusal_text) = prompt_feedback_refusal_text(chunk.prompt_feedback.as_ref())
            && !refusal_text.is_empty()
        {
            self.emit_message_delta(out, 0, refusal_text, true);
        }

        if let Some(model_status_message) = chunk
            .model_status
            .as_ref()
            .and_then(|status| status.message.as_ref())
            && !model_status_message.is_empty()
        {
            self.emit_message_delta(
                out,
                0,
                format!("model_status: {model_status_message}"),
                false,
            );
        }

        if let Some(candidates) = chunk.candidates {
            for (candidate_pos, candidate) in candidates.into_iter().enumerate() {
                let output_index = candidate.index.unwrap_or(candidate_pos as u32) as u64;

                if let Some(content) = candidate.content {
                    for (part_index, part) in content.parts.into_iter().enumerate() {
                        if part.thought.unwrap_or(false) {
                            if let Some(text) = part.text {
                                let item_id = part.thought_signature.unwrap_or_else(|| {
                                    format!(
                                        "reasoning_{}_{}_{}",
                                        output_index, self.chunk_sequence, part_index
                                    )
                                });
                                self.emit_reasoning_delta(out, output_index, item_id, text);
                            }
                            continue;
                        }

                        if let Some(function_call) = part.function_call {
                            let item_id = function_call.id.unwrap_or_else(|| {
                                format!(
                                    "tool_call_{}_{}_{}",
                                    output_index, self.chunk_sequence, part_index
                                )
                            });
                            let arguments_snapshot = function_call
                                .args
                                .as_ref()
                                .map(json_object_to_string)
                                .unwrap_or_else(|| "{}".to_string());
                            self.emit_function_call_snapshot(
                                out,
                                output_index,
                                item_id,
                                function_call.name,
                                arguments_snapshot,
                            );
                            continue;
                        }

                        if let Some(function_response) = part.function_response {
                            let output_text = gemini_function_response_to_text(function_response);
                            self.emit_message_delta(out, output_index, output_text, false);
                            continue;
                        }

                        if let Some(executable_code) = part.executable_code {
                            self.emit_message_delta(out, output_index, executable_code.code, false);
                            continue;
                        }

                        if let Some(code_execution_result) = part.code_execution_result {
                            if let Some(output_text) = code_execution_result.output {
                                self.emit_message_delta(out, output_index, output_text, false);
                            }
                            continue;
                        }

                        if let Some(text) = part.text {
                            self.emit_message_delta(out, output_index, text, false);
                            continue;
                        }

                        if let Some(inline_data) = part.inline_data {
                            self.emit_message_delta(
                                out,
                                output_index,
                                format!(
                                    "data:{};base64,{}",
                                    inline_data.mime_type, inline_data.data
                                ),
                                false,
                            );
                            continue;
                        }

                        if let Some(file_data) = part.file_data {
                            self.emit_message_delta(out, output_index, file_data.file_uri, false);
                        }
                    }
                }

                if let Some(finish_message) = candidate.finish_message
                    && !finish_message.is_empty()
                {
                    self.emit_message_delta(out, output_index, finish_message, false);
                }

                if let Some(finish_reason) = candidate.finish_reason.as_ref() {
                    if let Some(reason) = Self::map_finish_reason(finish_reason) {
                        self.incomplete_reason = Some(reason);
                    }
                    self.finish_output_index(out, output_index);
                }
            }
        }

        self.chunk_sequence = self.chunk_sequence.saturating_add(1);

        let in_progress_sequence = next_sequence_number(&mut self.next_sequence_number);
        push_stream_event(
            out,
            ResponseStreamEvent::InProgress {
                response: self.current_response(Some(rt::ResponseStatus::InProgress), None),
                sequence_number: in_progress_sequence,
            },
        );
    }

    fn close_all_open_items(&mut self, out: &mut Vec<ResponseStreamEvent>) {
        let output_indexes = self
            .message_items
            .keys()
            .chain(self.reasoning_items.keys())
            .copied()
            .collect::<Vec<_>>();
        for output_index in output_indexes {
            self.finish_output_index(out, output_index);
        }

        let remaining_call_ids = self.function_calls.keys().cloned().collect::<Vec<_>>();
        for call_id in remaining_call_ids {
            self.close_function_call(out, call_id);
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
        self.close_all_open_items(out);

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
}
fn output_text_part(text: String) -> ot::ResponseOutputText {
    ot::ResponseOutputText {
        annotations: Vec::new(),
        logprobs: None,
        text,
        type_: ot::ResponseOutputTextType::OutputText,
    }
}

fn refusal_part(refusal: String) -> ot::ResponseOutputRefusal {
    ot::ResponseOutputRefusal {
        refusal,
        type_: ot::ResponseOutputRefusalType::Refusal,
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
    refusal: bool,
) -> rt::ResponseOutputItem {
    rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
        id: item_id,
        content: if refusal {
            vec![ot::ResponseOutputContent::Refusal(refusal_part(text))]
        } else {
            vec![ot::ResponseOutputContent::Text(output_text_part(text))]
        },
        role: ot::ResponseOutputMessageRole::Assistant,
        phase: Some(ot::ResponseMessagePhase::FinalAnswer),
        status: Some(status),
        type_: Some(ot::ResponseOutputMessageType::Message),
    })
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
