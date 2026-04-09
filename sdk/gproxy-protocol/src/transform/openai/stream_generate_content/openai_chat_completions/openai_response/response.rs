use std::collections::{BTreeMap, BTreeSet};

use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::stream::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionChunkDelta,
    ChatCompletionChunkDeltaToolCall, ChatCompletionChunkDeltaToolCallType,
    ChatCompletionFunctionCallDelta,
};
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::response::ResponseBody as OpenAiCreateResponseBody;
use crate::openai::create_response::stream::{ResponseStreamEvent, ResponseStreamTokenLogprob};
use crate::openai::create_response::types as rt;
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
pub struct OpenAiResponseToOpenAiChatCompletionsStream {
    response_id: String,
    model: String,
    created: u64,
    service_tier: Option<ct::ChatCompletionServiceTier>,
    usage: Option<ct::CompletionUsage>,
    output_choice_map: BTreeMap<u64, u32>,
    role_emitted: BTreeSet<u32>,
    choice_tool_counts: BTreeMap<u32, u32>,
    choice_has_tool_calls: BTreeSet<u32>,
    choice_finish_reasons: BTreeMap<u32, ct::ChatCompletionFinishReason>,
    tool_states: BTreeMap<String, OpenAiChatToolState>,
    finished: bool,
}

impl OpenAiResponseToOpenAiChatCompletionsStream {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn map_service_tier(
        tier: Option<rt::ResponseServiceTier>,
    ) -> Option<ct::ChatCompletionServiceTier> {
        tier.map(|tier| match tier {
            rt::ResponseServiceTier::Auto => ct::ChatCompletionServiceTier::Auto,
            rt::ResponseServiceTier::Default => ct::ChatCompletionServiceTier::Default,
            rt::ResponseServiceTier::Flex => ct::ChatCompletionServiceTier::Flex,
            rt::ResponseServiceTier::Scale => ct::ChatCompletionServiceTier::Scale,
            rt::ResponseServiceTier::Priority => ct::ChatCompletionServiceTier::Priority,
        })
    }

    fn map_usage(usage: Option<rt::ResponseUsage>) -> Option<ct::CompletionUsage> {
        usage.map(|usage| ct::CompletionUsage {
            completion_tokens: usage.output_tokens,
            prompt_tokens: usage.input_tokens,
            total_tokens: usage.total_tokens,
            completion_tokens_details: Some(ct::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
                rejected_prediction_tokens: None,
            }),
            prompt_tokens_details: Some(ct::PromptTokensDetails {
                audio_tokens: None,
                cached_tokens: Some(usage.input_tokens_details.cached_tokens),
            }),
        })
    }

    fn update_metadata_from_response(&mut self, response: &OpenAiCreateResponseBody) {
        self.response_id = response.id.clone();
        self.model = response.model.clone();
        self.created = response.created_at;
        self.service_tier = Self::map_service_tier(response.service_tier.clone());
        self.usage = Self::map_usage(response.usage.clone());
    }

    fn default_finish_reason(
        &self,
        response: &OpenAiCreateResponseBody,
    ) -> ct::ChatCompletionFinishReason {
        match response
            .incomplete_details
            .as_ref()
            .and_then(|details| details.reason.as_ref())
        {
            Some(rt::ResponseIncompleteReason::MaxOutputTokens) => {
                ct::ChatCompletionFinishReason::Length
            }
            Some(rt::ResponseIncompleteReason::ContentFilter) => {
                ct::ChatCompletionFinishReason::ContentFilter
            }
            None => {
                if self.choice_has_tool_calls.is_empty() {
                    ct::ChatCompletionFinishReason::Stop
                } else {
                    ct::ChatCompletionFinishReason::ToolCalls
                }
            }
        }
    }

    fn fallback_response_id(&self) -> String {
        if self.response_id.is_empty() {
            "chatcmpl-stream".to_string()
        } else {
            self.response_id.clone()
        }
    }

    fn fallback_model(&self) -> String {
        if self.model.is_empty() {
            "chat.completion".to_string()
        } else {
            self.model.clone()
        }
    }

    fn chunk_event(
        &self,
        index: u32,
        delta: ChatCompletionChunkDelta,
        finish_reason: Option<ct::ChatCompletionFinishReason>,
        usage: Option<ct::CompletionUsage>,
        logprobs: Option<ct::ChatCompletionLogprobs>,
    ) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.fallback_response_id(),
            choices: vec![ChatCompletionChunkChoice {
                delta,
                finish_reason,
                index,
                logprobs,
            }],
            created: self.created,
            model: self.fallback_model(),
            object:
                crate::openai::create_chat_completions::stream::ChatCompletionChunkObject::ChatCompletionChunk,
            service_tier: self.service_tier.clone(),
            system_fingerprint: None,
            usage,
        }
    }

    fn map_output_text_annotation(
        annotation: serde_json::Value,
    ) -> Option<ct::ChatCompletionAnnotation> {
        match serde_json::from_value::<ot::ResponseOutputTextAnnotation>(annotation).ok()? {
            ot::ResponseOutputTextAnnotation::UrlCitation(citation) => {
                Some(ct::ChatCompletionAnnotation {
                    type_: ct::ChatCompletionAnnotationType::UrlCitation,
                    url_citation: ct::ChatCompletionUrlCitation {
                        start_index: citation.start_index,
                        end_index: citation.end_index,
                        title: citation.title,
                        url: citation.url,
                    },
                })
            }
            ot::ResponseOutputTextAnnotation::FileCitation(_)
            | ot::ResponseOutputTextAnnotation::ContainerFileCitation(_)
            | ot::ResponseOutputTextAnnotation::FilePath(_) => None,
        }
    }

    fn map_logprobs(
        logprobs: Option<Vec<ResponseStreamTokenLogprob>>,
    ) -> Option<ct::ChatCompletionLogprobs> {
        let logprobs = logprobs?;
        if logprobs.is_empty() {
            return None;
        }

        let content = logprobs
            .into_iter()
            .map(|entry| ct::ChatCompletionTokenLogprob {
                token: entry.token,
                bytes: None,
                logprob: entry.logprob,
                top_logprobs: entry
                    .top_logprobs
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|top| {
                        Some(ct::ChatCompletionTopLogprob {
                            token: top.token?,
                            bytes: None,
                            logprob: top.logprob.unwrap_or_default(),
                        })
                    })
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>();

        Some(ct::ChatCompletionLogprobs {
            content: Some(content),
            refusal: None,
        })
    }

    fn emit_error_refusal(&mut self, text: String, out: &mut Vec<ChatCompletionChunk>) {
        let choice_index = self.ensure_choice_index(0);
        self.maybe_emit_role(out, choice_index);
        out.push(self.chunk_event(
            choice_index,
            ChatCompletionChunkDelta {
                refusal: Some(text),
                ..Default::default()
            },
            None,
            None,
            None,
        ));
    }

    fn ensure_choice_index(&mut self, output_index: u64) -> u32 {
        if let Some(choice_index) = self.output_choice_map.get(&output_index) {
            return *choice_index;
        }
        let choice_index = u32::try_from(self.output_choice_map.len()).unwrap_or(u32::MAX);
        self.output_choice_map.insert(output_index, choice_index);
        choice_index
    }

    fn maybe_emit_role(&mut self, out: &mut Vec<ChatCompletionChunk>, choice_index: u32) {
        if self.role_emitted.insert(choice_index) {
            out.push(self.chunk_event(
                choice_index,
                ChatCompletionChunkDelta {
                    role: Some(ct::ChatCompletionDeltaRole::Assistant),
                    ..Default::default()
                },
                None,
                None,
                None,
            ));
        }
    }

    fn register_tool_call(
        &mut self,
        output_index: u64,
        call_id: String,
        name: String,
    ) -> OpenAiChatToolState {
        let choice_index = self.ensure_choice_index(output_index);
        let tool_index_ref = self.choice_tool_counts.entry(choice_index).or_insert(0);
        let tool_index = *tool_index_ref;
        *tool_index_ref = tool_index.saturating_add(1);
        self.choice_has_tool_calls.insert(choice_index);
        let state = OpenAiChatToolState {
            choice_index,
            tool_index,
            call_id: call_id.clone(),
            name,
            name_emitted: false,
        };
        self.tool_states.insert(call_id, state.clone());
        state
    }

    fn map_finish_reason_for_choice(
        &self,
        choice_index: u32,
        default_reason: ct::ChatCompletionFinishReason,
    ) -> ct::ChatCompletionFinishReason {
        self.choice_finish_reasons
            .get(&choice_index)
            .cloned()
            .unwrap_or_else(|| {
                if self.choice_has_tool_calls.contains(&choice_index) {
                    ct::ChatCompletionFinishReason::ToolCalls
                } else {
                    default_reason
                }
            })
    }

    fn sorted_choice_indexes(&self) -> Vec<u32> {
        let mut indexes = self.output_choice_map.values().copied().collect::<Vec<_>>();
        indexes.sort_unstable();
        indexes.dedup();
        indexes
    }

    pub fn on_stream_event(
        &mut self,
        event: ResponseStreamEvent,
        out: &mut Vec<ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        match event {
            ResponseStreamEvent::Created { response, .. }
            | ResponseStreamEvent::Queued { response, .. }
            | ResponseStreamEvent::InProgress { response, .. } => {
                self.update_metadata_from_response(&response);
            }
            ResponseStreamEvent::OutputItemAdded {
                item, output_index, ..
            } => match item {
                rt::ResponseOutputItem::Message(_) => {
                    let choice_index = self.ensure_choice_index(output_index);
                    self.maybe_emit_role(out, choice_index);
                }
                rt::ResponseOutputItem::FunctionToolCall(call) => {
                    let call_id = call.id.unwrap_or(call.call_id);
                    let state = self.register_tool_call(output_index, call_id, call.name);
                    self.maybe_emit_role(out, state.choice_index);
                    out.push(self.chunk_event(
                        state.choice_index,
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
                        None,
                    ));
                    if let Some(tool) = self.tool_states.get_mut(&state.call_id) {
                        tool.name_emitted = true;
                    }
                }
                rt::ResponseOutputItem::CustomToolCall(call) => {
                    let call_id = call.id.unwrap_or(call.call_id);
                    let state = self.register_tool_call(output_index, call_id, call.name);
                    self.maybe_emit_role(out, state.choice_index);
                    out.push(self.chunk_event(
                        state.choice_index,
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
                        None,
                    ));
                    if let Some(tool) = self.tool_states.get_mut(&state.call_id) {
                        tool.name_emitted = true;
                    }
                }
                _ => {}
            },
            ResponseStreamEvent::OutputTextDelta {
                output_index,
                delta,
                logprobs,
                obfuscation,
                ..
            } => {
                let choice_index = self.ensure_choice_index(output_index);
                self.maybe_emit_role(out, choice_index);
                out.push(self.chunk_event(
                    choice_index,
                    ChatCompletionChunkDelta {
                        content: Some(delta),
                        obfuscation,
                        ..Default::default()
                    },
                    None,
                    None,
                    Self::map_logprobs(logprobs),
                ));
            }
            ResponseStreamEvent::OutputTextDone {
                output_index,
                logprobs,
                ..
            } => {
                if let Some(mapped_logprobs) = Self::map_logprobs(logprobs) {
                    let choice_index = self.ensure_choice_index(output_index);
                    self.maybe_emit_role(out, choice_index);
                    out.push(self.chunk_event(
                        choice_index,
                        Default::default(),
                        None,
                        None,
                        Some(mapped_logprobs),
                    ));
                }
            }
            ResponseStreamEvent::OutputTextAnnotationAdded {
                output_index,
                annotation,
                ..
            } => {
                if let Some(mapped_annotation) = Self::map_output_text_annotation(annotation) {
                    let choice_index = self.ensure_choice_index(output_index);
                    self.maybe_emit_role(out, choice_index);
                    out.push(self.chunk_event(
                        choice_index,
                        ChatCompletionChunkDelta {
                            annotations: Some(vec![mapped_annotation]),
                            ..Default::default()
                        },
                        None,
                        None,
                        None,
                    ));
                }
            }
            ResponseStreamEvent::RefusalDelta {
                output_index,
                delta,
                obfuscation,
                ..
            } => {
                let choice_index = self.ensure_choice_index(output_index);
                self.maybe_emit_role(out, choice_index);
                out.push(self.chunk_event(
                    choice_index,
                    ChatCompletionChunkDelta {
                        refusal: Some(delta),
                        obfuscation,
                        ..Default::default()
                    },
                    None,
                    None,
                    None,
                ));
            }
            ResponseStreamEvent::ReasoningTextDelta {
                output_index,
                delta,
                obfuscation,
                ..
            }
            | ResponseStreamEvent::ReasoningSummaryTextDelta {
                output_index,
                delta,
                obfuscation,
                ..
            } => {
                let choice_index = self.ensure_choice_index(output_index);
                self.maybe_emit_role(out, choice_index);
                out.push(self.chunk_event(
                    choice_index,
                    ChatCompletionChunkDelta {
                        reasoning_content: Some(delta),
                        obfuscation,
                        ..Default::default()
                    },
                    None,
                    None,
                    None,
                ));
            }
            ResponseStreamEvent::FunctionCallArgumentsDelta { item_id, delta, .. }
            | ResponseStreamEvent::CustomToolCallInputDelta { item_id, delta, .. } => {
                if let Some(tool) = self.tool_states.get(&item_id).cloned() {
                    self.maybe_emit_role(out, tool.choice_index);
                    out.push(self.chunk_event(
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
                        None,
                    ));
                    if let Some(tool_state) = self.tool_states.get_mut(&item_id) {
                        tool_state.name_emitted = true;
                    }
                }
            }
            ResponseStreamEvent::OutputItemDone {
                item, output_index, ..
            } => {
                let choice_index = self.ensure_choice_index(output_index);
                match item {
                    rt::ResponseOutputItem::FunctionToolCall(_)
                    | rt::ResponseOutputItem::CustomToolCall(_) => {
                        self.choice_finish_reasons
                            .insert(choice_index, ct::ChatCompletionFinishReason::ToolCalls);
                    }
                    _ => {}
                }
            }
            ResponseStreamEvent::Completed { response, .. }
            | ResponseStreamEvent::Incomplete { response, .. }
            | ResponseStreamEvent::Failed { response, .. } => {
                self.update_metadata_from_response(&response);
                if !self.finished {
                    let default_reason = self.default_finish_reason(&response);
                    let mut choices = self.sorted_choice_indexes();
                    if choices.is_empty() {
                        choices.push(0);
                    }

                    for choice_index in &choices {
                        out.push(self.chunk_event(
                            *choice_index,
                            Default::default(),
                            Some(self.map_finish_reason_for_choice(
                                *choice_index,
                                default_reason.clone(),
                            )),
                            None,
                            None,
                        ));
                    }

                    if let Some(last) = out.last_mut() {
                        last.usage = self.usage.clone();
                    }

                    self.finished = true;
                }
            }
            ResponseStreamEvent::Error { error, .. } => {
                if !self.finished {
                    let detail = match error.param.as_deref() {
                        Some(param) => format!(
                            "openai_response_error code={} param={param} message={}",
                            error.code_or_type(),
                            error.message
                        ),
                        None => format!(
                            "openai_response_error code={} message={}",
                            error.code_or_type(),
                            error.message
                        ),
                    };
                    self.emit_error_refusal(detail, out);
                    self.finished = true;
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn finish(&mut self, _out: &mut Vec<ChatCompletionChunk>) -> Result<(), TransformError> {
        self.finished = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::create_response::stream::{
        ResponseStreamErrorPayload, ResponseStreamTopLogprob,
    };
    use crate::transform::openai::stream_generate_content::openai_response::utils::{
        response_snapshot, response_usage_from_counts,
    };
    use serde_json::json;

    #[test]
    fn response_stream_to_chat_stream_is_event_level() {
        let response_in_progress = response_snapshot(
            "resp_1",
            "gpt-5",
            Some(rt::ResponseStatus::InProgress),
            None,
            None,
            None,
            Some("hello".to_string()),
        );
        let response_done = response_snapshot(
            "resp_1",
            "gpt-5",
            Some(rt::ResponseStatus::Completed),
            Some(response_usage_from_counts(10, 2, 5, 0)),
            None,
            None,
            Some("hello".to_string()),
        );

        let mut converter = OpenAiResponseToOpenAiChatCompletionsStream::default();
        let mut converted = Vec::new();

        converter
            .on_stream_event(
                ResponseStreamEvent::Created {
                    response: response_in_progress,
                    sequence_number: 0,
                },
                &mut converted,
            )
            .expect("created");
        converter
            .on_stream_event(
                ResponseStreamEvent::OutputTextDelta {
                    content_index: 0,
                    delta: "hello".to_string(),
                    item_id: "msg_0".to_string(),
                    logprobs: None,
                    output_index: 0,
                    sequence_number: 1,
                    obfuscation: None,
                },
                &mut converted,
            )
            .expect("delta");
        converter
            .on_stream_event(
                ResponseStreamEvent::Completed {
                    response: response_done,
                    sequence_number: 2,
                },
                &mut converted,
            )
            .expect("completed");

        assert_eq!(converted.len(), 3);
        assert_eq!(
            converted[0].choices[0].delta.role,
            Some(ct::ChatCompletionDeltaRole::Assistant)
        );
        assert_eq!(converted[0].choices[0].delta.content, None);
        assert_eq!(converted[1].choices[0].delta.content.as_deref(), Some("hello"));
        assert_eq!(
            converted[2].choices[0].finish_reason,
            Some(ct::ChatCompletionFinishReason::Stop)
        );
        assert_eq!(
            converted[2].usage.as_ref().map(|usage| usage.total_tokens),
            Some(15)
        );
    }

    #[test]
    fn response_stream_maps_output_annotations_and_logprobs() {
        let response_in_progress = response_snapshot(
            "resp_annot",
            "gpt-5",
            Some(rt::ResponseStatus::InProgress),
            None,
            None,
            None,
            Some("hello".to_string()),
        );
        let response_done = response_snapshot(
            "resp_annot",
            "gpt-5",
            Some(rt::ResponseStatus::Completed),
            None,
            None,
            None,
            Some("hello".to_string()),
        );

        let mut converter = OpenAiResponseToOpenAiChatCompletionsStream::default();
        let mut converted = Vec::new();

        converter
            .on_stream_event(
                ResponseStreamEvent::Created {
                    response: response_in_progress,
                    sequence_number: 0,
                },
                &mut converted,
            )
            .expect("created");
        converter
            .on_stream_event(
                ResponseStreamEvent::OutputTextDelta {
                    content_index: 0,
                    delta: "hello".to_string(),
                    item_id: "msg_0".to_string(),
                    logprobs: Some(vec![ResponseStreamTokenLogprob {
                        token: "hello".to_string(),
                        logprob: -0.12,
                        top_logprobs: Some(vec![ResponseStreamTopLogprob {
                            token: Some("hello".to_string()),
                            logprob: Some(-0.12),
                        }]),
                    }]),
                    output_index: 0,
                    sequence_number: 1,
                    obfuscation: None,
                },
                &mut converted,
            )
            .expect("delta");
        converter
            .on_stream_event(
                ResponseStreamEvent::OutputTextAnnotationAdded {
                    annotation: json!({
                        "type": "url_citation",
                        "start_index": 0,
                        "end_index": 5,
                        "title": "https://example.com",
                        "url": "https://example.com"
                    }),
                    annotation_index: 0,
                    content_index: 0,
                    item_id: "msg_0".to_string(),
                    output_index: 0,
                    sequence_number: 2,
                },
                &mut converted,
            )
            .expect("annotation");
        converter
            .on_stream_event(
                ResponseStreamEvent::Completed {
                    response: response_done,
                    sequence_number: 3,
                },
                &mut converted,
            )
            .expect("completed");

        let saw_logprobs = converted.iter().any(|chunk| {
            chunk.choices
                .first()
                .and_then(|choice| choice.logprobs.as_ref())
                .and_then(|logprobs| logprobs.content.as_ref())
                .is_some_and(|content: &Vec<ct::ChatCompletionTokenLogprob>| !content.is_empty())
        });
        assert!(saw_logprobs);

        let mapped_url = converted.iter().find_map(|chunk| {
            chunk
                .choices
                .first()
                .and_then(|choice| choice.delta.annotations.as_ref())
                .and_then(|annotations: &Vec<ct::ChatCompletionAnnotation>| annotations.first())
                .map(|annotation| annotation.url_citation.url.clone())
        });
        assert_eq!(mapped_url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn response_stream_error_emits_detail_before_finish() {
        let mut converter = OpenAiResponseToOpenAiChatCompletionsStream::default();
        let mut converted = Vec::new();

        converter
            .on_stream_event(
                ResponseStreamEvent::Error {
                    error: ResponseStreamErrorPayload {
                        type_: "stream_error".to_string(),
                        code: Some("invalid_request_error".to_string()),
                        message: "bad input".to_string(),
                        param: Some("input".to_string()),
                    },
                    sequence_number: 0,
                },
                &mut converted,
            )
            .expect("error");

        let refusal_text = converted
            .iter()
            .find_map(|chunk| chunk.choices.first()?.delta.refusal.clone())
            .expect("expected refusal detail");
        assert!(refusal_text.contains("invalid_request_error"));
        assert!(refusal_text.contains("bad input"));
        assert!(refusal_text.contains("param=input"));
        assert!(converter.is_finished());
    }
}
