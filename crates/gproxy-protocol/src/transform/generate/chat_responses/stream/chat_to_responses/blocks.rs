use super::*;

impl ChatToResponsesStream {
    pub(super) fn message_mut(&mut self) -> Result<(), TransformError> {
        if self.message.is_some() {
            return Ok(());
        }
        let id = self
            .flow
            .resolve_as(
                IdentityRole::Message,
                IdentityRole::OutputItem(OutputItemKind::Message),
                SourceIdentity::new(Dialect::OpenAiChat, None, 0),
                &self.target_policy,
            )
            .map_err(|e| TransformError::invalid_result("identity.message", e.to_string()))?
            .emitted_id;
        if self.next_output as u64 >= self.limits.max_items as u64 {
            return Err(limit());
        }
        let index = self.next_output;
        self.next_output += 1;
        self.message = Some(MessageState {
            id: id.clone(),
            index,
            text: String::new(),
            refusal: String::new(),
            text_part: false,
            refusal_part: false,
            logs: None,
        });
        let item = r::ResponseOutputItem::Message(i::ResponseOutputMessage {
            id,
            content: Vec::new(),
            role: i::OutputMessageRole::Assistant,
            status: i::OutputMessageStatus::InProgress,
            phase: None,
            type_: i::MessageType::Message,
            rest: Default::default(),
        });
        self.emit(rs::StreamEvent::OutputItemAdded(rs::OutputItemEvent {
            sequence_number: self.sequence,
            output_index: index,
            item,
            rest: Default::default(),
        }))
    }

    pub(super) fn message_text(&mut self, text: String) -> Result<(), TransformError> {
        let (id, index, added) = {
            let state = self
                .message
                .as_mut()
                .ok_or_else(|| invalid("message not initialized"))?;
            let added = !state.text_part;
            state.text_part = true;
            state.text.push_str(&text);
            (state.id.clone(), state.index, added)
        };
        if added {
            self.emit(rs::StreamEvent::ContentPartAdded(rs::ContentPartEvent {
                sequence_number: self.sequence,
                item_id: id.clone(),
                output_index: index,
                content_index: 0,
                part: rs::OutputContentPart::Text(i::ResponseOutputText {
                    type_: i::ResponseOutputTextType::ResponseOutputText,
                    text: String::new(),
                    annotations: Vec::new(),
                    logprobs: Vec::new(),
                    rest: Default::default(),
                }),
                rest: Default::default(),
            }))?;
        }
        self.emit(rs::StreamEvent::OutputTextDelta(rs::OutputTextDelta {
            sequence_number: self.sequence,
            item_id: id,
            output_index: index,
            content_index: 0,
            delta: text,
            logprobs: Vec::new(),
            rest: Default::default(),
        }))
    }

    pub(super) fn tool_delta(
        &mut self,
        key: i64,
        source_id: Option<String>,
        function: cs::DeltaFunctionCall,
    ) -> Result<(), TransformError> {
        if !self.tools.contains_key(&key) && self.tools.len() >= self.limits.max_tool_calls {
            return Err(limit());
        }
        let t = self.tools.entry(key).or_insert_with(|| ToolState {
            source_id: None,
            name: String::new(),
            arguments: String::new(),
        });
        if let Some(id) = source_id {
            t.source_id = Some(id);
        }
        if let Some(name) = function.name.flatten() {
            t.name.push_str(&name);
        }
        if let Some(args) = function.arguments.flatten() {
            t.arguments.push_str(&args);
        }
        Ok(())
    }

    pub(super) fn finish_choice(&mut self) -> Result<(), TransformError> {
        let mut reasoning = rd::to_responses(&self.reasoning_details)?;
        if reasoning.is_empty() {
            let text = crate::wire::openai::chat::visible_reasoning(
                &Some(Some(std::mem::take(&mut self.reasoning_text))),
                &None,
                &Some(Some(std::mem::take(&mut self.reasoning_details))),
            );
            if let Some(text) = text.filter(|s| !s.is_empty()) {
                let id = self
                    .flow
                    .resolve_or_allocate(
                        IdentityRole::OutputItem(OutputItemKind::Reasoning),
                        SourceIdentity::new(Dialect::OpenAiChat, None, 0),
                        &self.target_policy,
                    )
                    .map_err(|e| invalid_owned(e.to_string()))?
                    .emitted_id;
                let mut item =
                    i::ReasoningItem::builder(i::ReasoningItemType::ReasoningItem, id, Vec::new())
                        .build();
                item.content = Some(vec![
                    i::ReasoningContent::builder(i::ReasoningTextType::ReasoningText, text).build(),
                ]);
                reasoning.push(item);
            }
        }
        for mut item in reasoning {
            if self.next_output as usize >= self.limits.max_items {
                return Err(limit());
            }
            let index = self.next_output;
            self.next_output += 1;
            item.status = Some(i::ReasoningStatus::InProgress);
            self.emit(rs::StreamEvent::OutputItemAdded(rs::OutputItemEvent {
                sequence_number: self.sequence,
                output_index: index,
                item: r::ResponseOutputItem::Reasoning(item.clone()),
                rest: Default::default(),
            }))?;
            item.status = Some(if self.finish == Some(c::FinishReason::Length) {
                i::ReasoningStatus::Incomplete
            } else {
                i::ReasoningStatus::Completed
            });
            let item = r::ResponseOutputItem::Reasoning(item);
            self.output.insert(index, item.clone());
            self.emit(rs::StreamEvent::OutputItemDone(rs::OutputItemEvent {
                sequence_number: self.sequence,
                output_index: index,
                item,
                rest: Default::default(),
            }))?;
        }
        let incomplete = matches!(
            self.finish,
            Some(c::FinishReason::Length | c::FinishReason::ContentFilter)
        );
        if let Some(state) = self.message.take() {
            let logs = crate::transform::generate::chat_responses::response::annotations::logs_to_responses(
                state.logs.as_ref().and_then(|v| v.content.clone()),
                &mut self.report,
            )?;
            if state.logs.as_ref().is_some_and(|v| v.refusal.is_some()) {
                self.report
                    .omitted("logprobs.refusal", "Responses refusal has no logprob field");
            }
            if !state.text_part && state.logs.as_ref().is_some_and(|v| v.content.is_some()) {
                return Err(invalid("logprobs without text"));
            }
            let stream_logs =
                crate::transform::generate::chat_responses::stream::common::stream_logs(&logs);
            if state.text_part {
                self.emit(rs::StreamEvent::OutputTextDone(rs::OutputTextDone {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index: 0,
                    text: state.text.clone(),
                    logprobs: stream_logs,
                    rest: Default::default(),
                }))?;
                self.emit(rs::StreamEvent::ContentPartDone(rs::ContentPartEvent {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index: 0,
                    part: rs::OutputContentPart::Text(i::ResponseOutputText {
                        type_: i::ResponseOutputTextType::ResponseOutputText,
                        text: state.text.clone(),
                        annotations: Vec::new(),
                        logprobs: logs.clone(),
                        rest: Default::default(),
                    }),
                    rest: Default::default(),
                }))?;
            }
            if state.refusal_part {
                let content_index = if state.text_part { 1 } else { 0 };
                self.emit(rs::StreamEvent::ContentPartAdded(rs::ContentPartEvent {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index,
                    part: rs::OutputContentPart::Refusal(
                        i::ResponseOutputRefusal::builder(
                            i::ResponseOutputRefusalType::ResponseOutputRefusal,
                            String::new(),
                        )
                        .build(),
                    ),
                    rest: Default::default(),
                }))?;
                self.emit(rs::StreamEvent::RefusalDelta(rs::RefusalDelta {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index,
                    delta: state.refusal.clone(),
                    rest: Default::default(),
                }))?;
                self.emit(rs::StreamEvent::RefusalDone(rs::RefusalDone {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index,
                    refusal: state.refusal.clone(),
                    rest: Default::default(),
                }))?;
                self.emit(rs::StreamEvent::ContentPartDone(rs::ContentPartEvent {
                    sequence_number: self.sequence,
                    item_id: state.id.clone(),
                    output_index: state.index,
                    content_index: if state.text_part { 1 } else { 0 },
                    part: rs::OutputContentPart::Refusal(i::ResponseOutputRefusal {
                        type_: i::ResponseOutputRefusalType::ResponseOutputRefusal,
                        refusal: state.refusal.clone(),
                        rest: Default::default(),
                    }),
                    rest: Default::default(),
                }))?;
            }
            let content = [
                state.text_part.then(|| {
                    i::OutputContent::Text(i::ResponseOutputText {
                        type_: i::ResponseOutputTextType::ResponseOutputText,
                        text: state.text,
                        annotations: Vec::new(),
                        logprobs: logs,
                        rest: Default::default(),
                    })
                }),
                state.refusal_part.then(|| {
                    i::OutputContent::Refusal(i::ResponseOutputRefusal {
                        type_: i::ResponseOutputRefusalType::ResponseOutputRefusal,
                        refusal: state.refusal,
                        rest: Default::default(),
                    })
                }),
            ]
            .into_iter()
            .flatten()
            .collect();
            let item = r::ResponseOutputItem::Message(i::ResponseOutputMessage {
                id: state.id.clone(),
                content,
                role: i::OutputMessageRole::Assistant,
                status: if incomplete {
                    i::OutputMessageStatus::Incomplete
                } else {
                    i::OutputMessageStatus::Completed
                },
                phase: None,
                type_: i::MessageType::Message,
                rest: Default::default(),
            });
            self.output.insert(state.index, item.clone());
            self.emit(rs::StreamEvent::OutputItemDone(rs::OutputItemEvent {
                sequence_number: self.sequence,
                output_index: state.index,
                item,
                rest: Default::default(),
            }))?;
        }
        let tools = std::mem::take(&mut self.tools);
        for (ordinal, (key, t)) in tools.into_iter().enumerate() {
            if (key != -1 && key != ordinal as i64)
                || self.next_output as u64 >= self.limits.max_items as u64
            {
                return Err(invalid("noncontiguous tool slots or item limit"));
            }
            let source =
                SourceIdentity::new(Dialect::OpenAiChat, t.source_id.clone(), ordinal as u64);
            let call_id = if key == -1 {
                self.flow
                    .resolve_legacy_chat_call(source, &self.target_policy)
            } else {
                self.flow
                    .resolve_or_allocate(IdentityRole::ToolCall, source, &self.target_policy)
            }
            .map_err(|e| invalid_owned(e.to_string()))?
            .emitted_id;
            let item_id = self
                .flow
                .resolve_as(
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(self.client_tools.kind(&t.name)),
                    SourceIdentity::new(Dialect::OpenAiChat, t.source_id, ordinal as u64),
                    &self.target_policy,
                )
                .map_err(|e| invalid_owned(e.to_string()))?
                .emitted_id;
            let output_index = self.next_output;
            self.next_output += 1;
            if key == -1 {
                self.report.changed(
                    "function_call",
                    "legacy call receives an invocation-scoped identity",
                );
            }
            if t.name.is_empty() {
                return Err(TransformError::missing_metadata("tool.function.name"));
            }
            if self.client_tools.kind(&t.name) != OutputItemKind::FunctionCall {
                // These native actions have no function-argument delta shape.
                // Emit only after the complete action can be validated, preserving
                // the original client executor and call/item identity roles.
                let item = self.client_tools.restore(i::FunctionCall {
                    async_: None,
                    type_: i::FunctionCallType::FunctionCall,
                    arguments: t.arguments,
                    call_id,
                    name: t.name,
                    id: Some(item_id),
                    namespace: None,
                    caller: None,
                    status: Some(if incomplete {
                        i::ItemStatus::Incomplete
                    } else {
                        i::ItemStatus::Completed
                    }),
                    rest: Default::default(),
                })?;
                self.emit(rs::StreamEvent::OutputItemAdded(rs::OutputItemEvent {
                    sequence_number: self.sequence,
                    output_index,
                    item: item.clone(),
                    rest: Default::default(),
                }))?;
                self.output.insert(output_index, item.clone());
                self.emit(rs::StreamEvent::OutputItemDone(rs::OutputItemEvent {
                    sequence_number: self.sequence,
                    output_index,
                    item,
                    rest: Default::default(),
                }))?;
                continue;
            }
            let item = self.client_tools.restore(i::FunctionCall {
                async_: None,
                type_: i::FunctionCallType::FunctionCall,
                arguments: String::new(),
                call_id: call_id.clone(),
                name: t.name.clone(),
                id: Some(item_id.clone()),
                namespace: None,
                caller: None,
                status: Some(i::ItemStatus::InProgress),
                rest: Default::default(),
            })?;
            let r::ResponseOutputItem::FunctionCall(ref restored) = item else {
                // restore() returns the same FunctionCall item it was handed.
                unreachable!()
            };
            let restored_name = restored.name.clone();
            self.emit(rs::StreamEvent::OutputItemAdded(rs::OutputItemEvent {
                sequence_number: self.sequence,
                output_index,
                item,
                rest: Default::default(),
            }))?;
            self.emit(rs::StreamEvent::FunctionCallArgumentsDelta(
                rs::FunctionCallArgumentsDelta {
                    sequence_number: self.sequence,
                    item_id: item_id.clone(),
                    output_index,
                    delta: t.arguments.clone(),
                    rest: Default::default(),
                },
            ))?;
            self.emit(rs::StreamEvent::FunctionCallArgumentsDone(
                rs::FunctionCallArgumentsDone {
                    sequence_number: self.sequence,
                    item_id: item_id.clone(),
                    output_index,
                    name: restored_name,
                    arguments: t.arguments.clone(),
                    rest: Default::default(),
                },
            ))?;
            let item = self.client_tools.restore(i::FunctionCall {
                async_: None,
                type_: i::FunctionCallType::FunctionCall,
                arguments: t.arguments,
                call_id,
                name: t.name,
                id: Some(item_id.clone()),
                namespace: None,
                caller: None,
                status: Some(if incomplete {
                    i::ItemStatus::Incomplete
                } else {
                    i::ItemStatus::Completed
                }),
                rest: Default::default(),
            })?;
            self.output.insert(output_index, item.clone());
            self.emit(rs::StreamEvent::OutputItemDone(rs::OutputItemEvent {
                sequence_number: self.sequence,
                output_index,
                item,
                rest: Default::default(),
            }))?;
        }
        Ok(())
    }
}
