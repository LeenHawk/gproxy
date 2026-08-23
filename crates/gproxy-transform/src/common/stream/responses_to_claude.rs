use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;
use crate::common::usage;
use crate::envelope::SseFrame;
use crate::models::common::wire_string;

use super::openai_to_claude::{Scalar, State, item_id};

impl State {
    pub(super) fn responses(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let event: openai::ResponseStreamEvent = serde_json::from_str(&frame.data)?;
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Err(TransformError::unsupported(
                "Responses stream event",
                frame.data,
            ));
        };
        let event = *event;
        if let Some(response) = event.response.as_ref() {
            self.id = Some(response.id.clone());
            if let Some(model) = response.model.as_ref() {
                self.model = Some(wire_string(model)?.into());
            }
        }
        let message_rest = event
            .response
            .as_ref()
            .map(|response| response.rest.clone())
            .unwrap_or_default();
        let mut output = self.ensure_start(message_rest, event.rest.clone())?;
        match event.type_ {
            openai::ResponseStreamEventTypeKnown::ResponseCreated => {}
            openai::ResponseStreamEventTypeKnown::ResponseInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseQueued => {
                if let Some(response) = event.response {
                    self.pending_rest.extend(response.rest);
                }
                self.pending_rest.extend(event.rest);
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded => {
                let item = event
                    .item
                    .ok_or_else(|| TransformError::shape("Responses stream", "item is missing"))?;
                let output_index = event.output_index.ok_or_else(|| {
                    TransformError::shape("Responses stream", "output_index is missing")
                })?;
                let native_id = match item.as_ref() {
                    openai::ResponseItem::Typed(item) => items::item_id(item),
                    _ => None,
                };
                let item_id = item_id(&item).or(native_id);
                match *item {
                    openai::ResponseItem::Typed(item) => match *item {
                        openai::TypedResponseItem::FunctionCall {
                            id,
                            call_id,
                            name,
                            arguments,
                            mut rest,
                            ..
                        } => {
                            if let Some(id) = id {
                                rest.insert("openai_item_id".into(), id.into());
                            }
                            output.extend(self.response_tool(
                                item_id.clone().ok_or_else(|| {
                                    TransformError::shape(
                                        "Responses stream",
                                        "function item id is missing",
                                    )
                                })?,
                                call_id,
                                name,
                                arguments,
                                rest,
                                event.rest,
                            )?)
                        }
                        openai::TypedResponseItem::CustomToolCall {
                            id,
                            call_id,
                            name,
                            input,
                            mut rest,
                            ..
                        } => {
                            if let Some(id) = id {
                                rest.insert("openai_item_id".into(), id.into());
                            }
                            output.extend(self.response_tool(
                                item_id.clone().ok_or_else(|| {
                                    TransformError::shape(
                                        "Responses stream",
                                        "custom item id is missing",
                                    )
                                })?,
                                call_id,
                                name,
                                input,
                                rest,
                                event.rest,
                            )?)
                        }
                        openai::TypedResponseItem::Reasoning {
                            encrypted_content,
                            rest,
                            ..
                        } => {
                            let item_id = item_id.clone().ok_or_else(|| {
                                TransformError::shape(
                                    "Responses stream",
                                    "reasoning item id is missing",
                                )
                            })?;
                            let index = self.allocate();
                            output.extend(self.block_start(
                                index,
                                claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                                    signature: encrypted_content,
                                    thinking: String::new(),
                                    type_: claude::ThinkingBlockType::Thinking,
                                    rest,
                                }),
                                event.rest,
                            )?);
                            self.response_indices.insert((item_id, None), index);
                        }
                        other => {
                            if let Some(call) = items::openai_call(other.clone())? {
                                let index = self.allocate();
                                output.extend(self.block_start(
                                    index,
                                    items::response_block(call),
                                    event.rest,
                                )?);
                                if let Some(item_id) = item_id {
                                    self.response_indices.insert((item_id, None), index);
                                }
                                self.response_output_indices
                                    .entry(output_index)
                                    .or_default()
                                    .push(index);
                                self.has_tool = true;
                            } else {
                                return Err(TransformError::unsupported(
                                    "Responses output item",
                                    serde_json::to_string(&other)?,
                                ));
                            }
                        }
                    },
                    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                        if !message.content.is_empty() {
                            return Err(TransformError::unsupported(
                                "Responses output item",
                                "message content before content-part events",
                            ));
                        }
                    }
                    openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_)) => {
                        return Err(TransformError::unsupported(
                            "Responses output item",
                            "input message in output stream",
                        ));
                    }
                    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_)) => {
                        return Err(TransformError::unsupported(
                            "Responses output item",
                            "easy input message in output stream",
                        ));
                    }
                    openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(raw)) => {
                        return Err(TransformError::unsupported(
                            "Responses output message",
                            raw.to_string(),
                        ));
                    }
                    openai::ResponseItem::Unknown(raw) => {
                        return Err(TransformError::unsupported(
                            "Responses output item",
                            raw.to_string(),
                        ));
                    }
                }
            }
            openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded => {
                let item_id = event.item_id.clone().ok_or_else(|| {
                    TransformError::shape("Responses stream", "item_id is missing")
                })?;
                let content_index = event.content_index.ok_or_else(|| {
                    TransformError::shape("Responses stream", "content_index is missing")
                })?;
                let part = event.part.ok_or_else(|| {
                    TransformError::shape("Responses stream", "content part is missing")
                })?;
                match part {
                    openai::ResponseContentPart::OutputText(part) => {
                        let index = self.allocate();
                        let text = part.text;
                        output.extend(self.block_start(
                            index,
                            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                                citations: None,
                                text: String::new(),
                                type_: claude::TextBlockType::Text,
                                rest: part.rest,
                            }),
                            event.rest,
                        )?);
                        self.response_indices
                            .insert((item_id, Some(content_index)), index);
                        if !text.is_empty() {
                            output.push(self.delta(
                                index,
                                claude::KnownEventDelta::Text {
                                    text,
                                    rest: Default::default(),
                                },
                            )?);
                        }
                    }
                    openai::ResponseContentPart::Refusal(part) => {
                        let index = self.allocate();
                        let refusal = part.refusal;
                        output.extend(self.block_start(
                            index,
                            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                                citations: None,
                                text: String::new(),
                                type_: claude::TextBlockType::Text,
                                rest: part.rest,
                            }),
                            event.rest,
                        )?);
                        self.response_indices
                            .insert((item_id, Some(content_index)), index);
                        if !refusal.is_empty() {
                            output.push(self.delta(
                                index,
                                claude::KnownEventDelta::Text {
                                    text: refusal,
                                    rest: Default::default(),
                                },
                            )?);
                        }
                    }
                    openai::ResponseContentPart::ReasoningText(part) => {
                        let index = self.response_index(Some(&item_id), None)?;
                        self.response_indices
                            .insert((item_id, Some(content_index)), index);
                        if !part.text.is_empty() {
                            output.push(self.delta(
                                index,
                                claude::KnownEventDelta::Thinking {
                                    estimated_tokens: None,
                                    thinking: part.text,
                                    rest: part.rest,
                                },
                            )?);
                        }
                    }
                    openai::ResponseContentPart::Unknown(raw) => {
                        return Err(TransformError::unsupported(
                            "Responses content part",
                            raw.to_string(),
                        ));
                    }
                }
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta => {
                output.extend(self.response_scalar(event, Scalar::Text)?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDelta => {
                output.extend(self.response_scalar(event, Scalar::Thinking)?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta => {
                let index = self.response_index(event.item_id.as_deref(), None)?;
                output.push(self.input_delta(
                    index,
                    event.delta.unwrap_or_default(),
                    event.rest,
                )?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta => {
                let index = self.response_index(event.item_id.as_deref(), None)?;
                output.push(self.input_delta(
                    index,
                    event.delta.unwrap_or_default(),
                    event.rest,
                )?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseRefusalDelta => {
                let mut event = event;
                event.delta = event.delta.take().or(event.refusal.take());
                output.extend(self.response_scalar(event, Scalar::Text)?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseContentPartDone => {
                self.response_index(event.item_id.as_deref(), event.content_index)?;
                self.pending_rest.extend(event.rest);
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDone
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDone
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDone
            | openai::ResponseStreamEventTypeKnown::ResponseRefusalDone => {
                self.response_index(event.item_id.as_deref(), event.content_index)?;
                self.pending_rest.extend(event.rest);
            }
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone
            | openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone => {
                self.response_index(event.item_id.as_deref(), None)?;
                self.pending_rest.extend(event.rest);
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone => {
                let id = event
                    .item_id
                    .or_else(|| event.item.as_deref().and_then(item_id));
                let indices = if let Some(id) = id {
                    self.response_indices
                        .iter()
                        .filter_map(|((candidate, _), index)| (candidate == &id).then_some(*index))
                        .collect::<Vec<_>>()
                } else {
                    let output_index = event.output_index.ok_or_else(|| {
                        TransformError::shape(
                            "Responses stream",
                            "done item has neither id nor output_index",
                        )
                    })?;
                    self.response_output_indices
                        .remove(&output_index)
                        .ok_or_else(|| {
                            TransformError::shape(
                                "Responses stream",
                                "output item done before represented content",
                            )
                        })?
                };
                if indices.is_empty() {
                    return Err(TransformError::shape(
                        "Responses stream",
                        "output item done before represented content",
                    ));
                }
                for (position, index) in indices.iter().enumerate() {
                    let rest = if position + 1 == indices.len() {
                        event.rest.clone()
                    } else {
                        Default::default()
                    };
                    output.extend(self.close(*index, rest)?);
                }
            }
            openai::ResponseStreamEventTypeKnown::ResponseCompleted => {
                let mut terminal_rest = std::mem::take(&mut self.pending_rest);
                terminal_rest.extend(event.rest);
                let response = event.response.ok_or_else(|| {
                    TransformError::shape("Responses stream", "terminal response is missing")
                })?;
                if self.next_index == 0 && !response.output.is_empty() {
                    let converted = crate::generate_content::claude_messages_to_openai_responses::response::transform(
                        Bytes::from(serde_json::to_vec(response.as_ref())?),
                    )?;
                    let message: claude::CreateMessageResponseBody =
                        serde_json::from_slice(&converted)?;
                    for block in message.content {
                        self.has_tool |= matches!(block, claude::ResponseContentBlock::ToolUse(_));
                        let index = self.allocate();
                        output.extend(self.block_start(index, block, Default::default())?);
                        output.extend(self.close(index, Default::default())?);
                    }
                }
                output.extend(self.finish_message(
                    claude::StopReason::Known(if self.has_tool {
                        claude::StopReasonKnown::ToolUse
                    } else {
                        claude::StopReasonKnown::EndTurn
                    }),
                    usage::responses_to_claude(response.usage),
                    true,
                    terminal_rest,
                )?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseIncomplete => {
                let mut terminal_rest = std::mem::take(&mut self.pending_rest);
                terminal_rest.extend(event.rest);
                let usage =
                    usage::responses_to_claude(event.response.and_then(|response| response.usage));
                output.extend(self.finish_message(
                    claude::StopReason::Known(claude::StopReasonKnown::MaxTokens),
                    usage,
                    true,
                    terminal_rest,
                )?);
            }
            openai::ResponseStreamEventTypeKnown::ResponseFailed
            | openai::ResponseStreamEventTypeKnown::Error => {
                return Err(TransformError::unsupported(
                    "Responses stream",
                    "failed response",
                ));
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextAnnotationAdded
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryPartAdded
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryPartDone
            | openai::ResponseStreamEventTypeKnown::ResponseAudioDelta
            | openai::ResponseStreamEventTypeKnown::ResponseAudioDone
            | openai::ResponseStreamEventTypeKnown::ResponseAudioTranscriptDelta
            | openai::ResponseStreamEventTypeKnown::ResponseAudioTranscriptDone
            | openai::ResponseStreamEventTypeKnown::ResponseImageGenerationCallCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseImageGenerationCallGenerating
            | openai::ResponseStreamEventTypeKnown::ResponseImageGenerationCallInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseImageGenerationCallPartialImage
            | openai::ResponseStreamEventTypeKnown::ResponseFileSearchCallInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseFileSearchCallSearching
            | openai::ResponseStreamEventTypeKnown::ResponseFileSearchCallCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseWebSearchCallInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseWebSearchCallSearching
            | openai::ResponseStreamEventTypeKnown::ResponseWebSearchCallCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseCodeInterpreterCallInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseCodeInterpreterCallInterpreting
            | openai::ResponseStreamEventTypeKnown::ResponseCodeInterpreterCallCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseCodeInterpreterCallCodeDelta
            | openai::ResponseStreamEventTypeKnown::ResponseCodeInterpreterCallCodeDone
            | openai::ResponseStreamEventTypeKnown::ResponseMcpCallArgumentsDelta
            | openai::ResponseStreamEventTypeKnown::ResponseMcpCallArgumentsDone
            | openai::ResponseStreamEventTypeKnown::ResponseMcpCallInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseMcpCallCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseMcpCallFailed
            | openai::ResponseStreamEventTypeKnown::ResponseMcpListToolsInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseMcpListToolsCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseMcpListToolsFailed => {
                return Err(TransformError::unsupported(
                    "Responses stream event",
                    event.type_.as_str(),
                ));
            }
        }
        Ok(output)
    }

    fn response_tool(
        &mut self,
        item_id: String,
        id: String,
        name: String,
        input: String,
        rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let index = self.allocate();
        let mut output = self.block_start(
            index,
            claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id,
                input: Default::default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest,
            }),
            event_rest,
        )?;
        self.response_indices.insert((item_id, None), index);
        self.has_tool = true;
        if !input.is_empty() {
            output.push(self.input_delta(index, input, Default::default())?);
        }
        Ok(output)
    }
}
