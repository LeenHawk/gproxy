use super::*;

pub(super) struct Emitter {
    pub events: Vec<s::StreamEvent>,
    collector: Option<ResponsesStreamCollector>,
    sequence: i64,
    agent: Option<crate::wire::openai::responses::multi_agent::Agent>,
}

impl Emitter {
    pub fn new(limits: ResponsesStreamLimits) -> Self {
        Self {
            events: Vec::new(),
            collector: Some(ResponsesStreamCollector::new(limits)),
            sequence: 0,
            agent: None,
        }
    }
    pub fn push(&mut self, f: impl FnOnce(i64) -> s::StreamEvent) -> Result<(), TransformError> {
        let collector = self
            .collector
            .as_ref()
            .ok_or_else(|| invalid("emitter finished"))?;

        let mut event = f(self.sequence);
        event.set_agent(self.agent.clone());
        bounded(
            &event,
            collector.limits.max_bytes.saturating_sub(collector.bytes),
        )?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(limit)?;
        self.collector
            .as_mut()
            .ok_or_else(|| invalid("emitter finished"))?
            .push(event.clone())?;
        self.events.push(event);
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), TransformError> {
        self.collector
            .take()
            .ok_or_else(|| invalid("emitter finished"))?
            .finish()?;
        Ok(())
    }
    pub fn item(&mut self, index: i64, item: &r::ResponseOutputItem) -> Result<(), TransformError> {
        self.agent = item.agent().cloned();
        let mut start = item.clone();
        match &mut start {
            r::ResponseOutputItem::Message(v) => {
                v.content.clear();
                v.status = i::OutputMessageStatus::InProgress;
            }
            r::ResponseOutputItem::Reasoning(v) => {
                v.summary.clear();
                v.content = v.content.as_ref().map(|_| Vec::new());
                v.encrypted_content = None;
                v.status = Some(i::ReasoningStatus::InProgress);
            }
            r::ResponseOutputItem::FunctionCall(v) => {
                v.arguments.clear();
                v.status = Some(i::ItemStatus::InProgress);
            }
            r::ResponseOutputItem::CustomToolCall(v) => v.input.clear(),
            r::ResponseOutputItem::McpCall(v) => {
                v.arguments.clear();
                v.output = None;
                v.error = None;
                v.status = Some(i::McpCallStatus::InProgress);
            }
            r::ResponseOutputItem::CodeInterpreterCall(v) => {
                v.code = v.code.as_ref().map(|_| String::new());
                v.outputs = None;
                v.status = i::CodeInterpreterStatus::InProgress;
            }
            _ => {}
        }
        self.push(|sequence_number| {
            s::StreamEvent::OutputItemAdded(s::OutputItemEvent {
                agent: None,
                sequence_number,
                output_index: index,
                item: start,
                rest: Default::default(),
            })
        })?;
        let id = super::items::item_id(item)
            .ok_or_else(|| invalid("missing output item identity after allocation"))?;
        match item {
            r::ResponseOutputItem::Message(v) => {
                for (content_index, p) in v.content.iter().enumerate() {
                    let part = match p {
                        i::OutputContent::Text(v) => s::OutputContentPart::Text(v.clone()),
                        i::OutputContent::Refusal(v) => s::OutputContentPart::Refusal(v.clone()),
                    };
                    self.part(index, id, content_index as i64, part, false)?;
                }
            }
            r::ResponseOutputItem::Reasoning(v) => {
                for (content_index, p) in v.summary.iter().enumerate() {
                    self.part(
                        index,
                        id,
                        content_index as i64,
                        s::OutputContentPart::Reasoning(s::ReasoningText {
                            text: p.text.clone(),
                            type_: s::ReasoningTextType::ReasoningText,
                            rest: Default::default(),
                        }),
                        true,
                    )?;
                }
                for (content_index, p) in v.content.iter().flatten().enumerate() {
                    self.part(
                        index,
                        id,
                        content_index as i64,
                        s::OutputContentPart::Reasoning(s::ReasoningText {
                            text: p.text.clone(),
                            type_: s::ReasoningTextType::ReasoningText,
                            rest: Default::default(),
                        }),
                        false,
                    )?;
                }
            }
            r::ResponseOutputItem::FunctionCall(v) => {
                self.push(|sequence_number| {
                    s::StreamEvent::FunctionCallArgumentsDelta(s::FunctionCallArgumentsDelta {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        delta: v.arguments.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.push(|sequence_number| {
                    s::StreamEvent::FunctionCallArgumentsDone(s::FunctionCallArgumentsDone {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        name: v.name.clone(),
                        arguments: v.arguments.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::CustomToolCall(v) => {
                self.push(|sequence_number| {
                    s::StreamEvent::CustomToolInputDelta(s::CustomToolInputDelta {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        delta: v.input.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.push(|sequence_number| {
                    s::StreamEvent::CustomToolInputDone(s::CustomToolInputDone {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        input: v.input.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::McpCall(v) => {
                self.push(|sequence_number| {
                    s::StreamEvent::McpArgumentsDelta(s::McpArgumentsDelta {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        delta: v.arguments.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.push(|sequence_number| {
                    s::StreamEvent::McpArgumentsDone(s::McpArgumentsDone {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        arguments: v.arguments.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::CodeInterpreterCall(v) => {
                if let Some(code) = &v.code {
                    self.push(|sequence_number| {
                        s::StreamEvent::CodeInterpreterCodeDelta(s::CodeInterpreterCodeDelta {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            delta: code.clone(),
                            rest: Default::default(),
                        })
                    })?;
                    self.push(|sequence_number| {
                        s::StreamEvent::CodeInterpreterCodeDone(s::CodeInterpreterCodeDone {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            code: code.clone(),
                            rest: Default::default(),
                        })
                    })?;
                }
            }
            _ => {}
        }
        self.push(|sequence_number| {
            s::StreamEvent::OutputItemDone(s::OutputItemEvent {
                agent: None,
                sequence_number,
                output_index: index,
                item: item.clone(),
                rest: Default::default(),
            })
        })
    }
    pub fn part(
        &mut self,
        index: i64,
        id: &str,
        content: i64,
        part: s::OutputContentPart,
        summary: bool,
    ) -> Result<(), TransformError> {
        let mut start = part.clone();
        match &mut start {
            s::OutputContentPart::Text(v) => {
                v.text.clear();
                v.annotations.clear();
                v.logprobs.clear();
            }
            s::OutputContentPart::Refusal(v) => v.refusal.clear(),
            s::OutputContentPart::Reasoning(v) => v.text.clear(),
        }
        if summary {
            self.push(|sequence_number| {
                s::StreamEvent::ReasoningSummaryPartAdded(s::ReasoningSummaryPartAddedEvent {
                    agent: None,
                    sequence_number,
                    item_id: id.into(),
                    output_index: index,
                    summary_index: content,
                    part: s::ReasoningSummaryPart {
                        text: String::new(),
                        type_: s::ReasoningSummaryPartType::SummaryText,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                })
            })?;
        } else {
            self.push(|sequence_number| {
                s::StreamEvent::ContentPartAdded(s::ContentPartEvent {
                    agent: None,
                    sequence_number,
                    item_id: id.into(),
                    output_index: index,
                    content_index: content,
                    part: start,
                    rest: Default::default(),
                })
            })?;
        }
        match &part {
            s::OutputContentPart::Text(v) => {
                let logs = super::parts::stream_logs(&v.logprobs);
                self.push(|sequence_number| {
                    s::StreamEvent::OutputTextDelta(s::OutputTextDelta {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        content_index: content,
                        delta: v.text.clone(),
                        logprobs: logs.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.push(|sequence_number| {
                    s::StreamEvent::OutputTextDone(s::OutputTextDone {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        content_index: content,
                        text: v.text.clone(),
                        logprobs: logs,
                        rest: Default::default(),
                    })
                })?;
            }
            s::OutputContentPart::Refusal(v) => {
                self.push(|sequence_number| {
                    s::StreamEvent::RefusalDelta(s::RefusalDelta {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        content_index: content,
                        delta: v.refusal.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.push(|sequence_number| {
                    s::StreamEvent::RefusalDone(s::RefusalDone {
                        agent: None,
                        sequence_number,
                        item_id: id.into(),
                        output_index: index,
                        content_index: content,
                        refusal: v.refusal.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            s::OutputContentPart::Reasoning(v) => {
                if summary {
                    self.push(|sequence_number| {
                        s::StreamEvent::ReasoningSummaryTextDelta(s::ReasoningSummaryTextDelta {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            summary_index: content,
                            delta: v.text.clone(),
                            rest: Default::default(),
                        })
                    })?;
                    self.push(|sequence_number| {
                        s::StreamEvent::ReasoningSummaryTextDone(s::ReasoningSummaryTextDone {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            summary_index: content,
                            text: v.text.clone(),
                            rest: Default::default(),
                        })
                    })?;
                } else {
                    self.push(|sequence_number| {
                        s::StreamEvent::ReasoningTextDelta(s::ReasoningTextDelta {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            content_index: content,
                            delta: v.text.clone(),
                            rest: Default::default(),
                        })
                    })?;
                    self.push(|sequence_number| {
                        s::StreamEvent::ReasoningTextDone(s::ReasoningTextDone {
                            agent: None,
                            sequence_number,
                            item_id: id.into(),
                            output_index: index,
                            content_index: content,
                            text: v.text.clone(),
                            rest: Default::default(),
                        })
                    })?;
                }
            }
        }
        if summary {
            let s::OutputContentPart::Reasoning(v) = part else {
                return Err(invalid("summary type mismatch"));
            };
            self.push(|sequence_number| {
                s::StreamEvent::ReasoningSummaryPartDone(s::ReasoningSummaryPartDoneEvent {
                    agent: None,
                    sequence_number,
                    item_id: id.into(),
                    output_index: index,
                    summary_index: content,
                    part: s::ReasoningSummaryPart {
                        text: v.text,
                        type_: s::ReasoningSummaryPartType::SummaryText,
                        rest: Default::default(),
                    },
                    status: None,
                    rest: Default::default(),
                })
            })
        } else {
            self.push(|sequence_number| {
                s::StreamEvent::ContentPartDone(s::ContentPartEvent {
                    agent: None,
                    sequence_number,
                    item_id: id.into(),
                    output_index: index,
                    content_index: content,
                    part,
                    rest: Default::default(),
                })
            })
        }
    }
}
