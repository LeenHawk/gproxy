use super::common::{Budget, StreamLimits, invalid, limit};
use crate::{
    transform::TransformError,
    transform::generate::stream::responses::{ResponsesStreamCollector, ResponsesStreamLimits},
    wire::openai::responses::{input as i, response as r, stream as s},
};

pub(super) struct ResponseEvents {
    native: Option<ResponsesStreamCollector>,
    sequence: i64,
}

impl ResponseEvents {
    pub fn new(limits: StreamLimits) -> Self {
        Self {
            native: Some(ResponsesStreamCollector::new(ResponsesStreamLimits {
                max_events: limits.max_events,
                max_bytes: limits.max_bytes,
                max_items: limits.max_items,
                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            })),
            sequence: 0,
        }
    }
    pub fn emit(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        make: impl FnOnce(i64) -> s::StreamEvent,
    ) -> Result<(), TransformError> {
        let event = make(self.sequence);
        budget.output(&event)?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(limit)?;
        self.native
            .as_mut()
            .ok_or_else(|| invalid("target consumed"))?
            .push(event.clone())?;
        out.push(event);
        Ok(())
    }
    pub fn finish(mut self) -> Result<r::GenerateContentResponseBody, TransformError> {
        Ok(self
            .native
            .take()
            .ok_or_else(|| invalid("target consumed"))?
            .finish()?
            .value)
    }
    pub fn part_added(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        index: i64,
        id: String,
        reasoning: bool,
    ) -> Result<(), TransformError> {
        let part = if reasoning {
            s::OutputContentPart::Reasoning(s::ReasoningText {
                type_: s::ReasoningTextType::ReasoningText,
                text: String::new(),
                rest: Default::default(),
            })
        } else {
            s::OutputContentPart::Text(text(String::new(), Vec::new()))
        };
        self.emit(budget, out, |sequence_number| {
            s::StreamEvent::ContentPartAdded(s::ContentPartEvent {
                sequence_number,
                item_id: id,
                output_index: index,
                content_index: 0,
                part,
                rest: Default::default(),
            })
        })
    }
    pub fn text(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        index: i64,
        id: String,
        value: String,
        reasoning: bool,
    ) -> Result<(), TransformError> {
        self.emit(budget, out, |sequence_number| {
            if reasoning {
                s::StreamEvent::ReasoningTextDelta(s::ReasoningTextDelta {
                    sequence_number,
                    item_id: id,
                    output_index: index,
                    content_index: 0,
                    delta: value,
                    rest: Default::default(),
                })
            } else {
                s::StreamEvent::OutputTextDelta(s::OutputTextDelta {
                    sequence_number,
                    item_id: id,
                    output_index: index,
                    content_index: 0,
                    delta: value,
                    logprobs: Vec::new(),
                    rest: Default::default(),
                })
            }
        })
    }
    pub fn arguments(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        index: i64,
        id: String,
        value: String,
        mcp: bool,
    ) -> Result<(), TransformError> {
        self.emit(budget, out, |sequence_number| {
            if mcp {
                s::StreamEvent::McpArgumentsDelta(s::McpArgumentsDelta {
                    sequence_number,
                    item_id: id,
                    output_index: index,
                    delta: value,
                    rest: Default::default(),
                })
            } else {
                s::StreamEvent::FunctionCallArgumentsDelta(s::FunctionCallArgumentsDelta {
                    sequence_number,
                    item_id: id,
                    output_index: index,
                    delta: value,
                    rest: Default::default(),
                })
            }
        })
    }
    pub fn arguments_done(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        index: i64,
        item: &r::ResponseOutputItem,
    ) -> Result<(), TransformError> {
        match item {
            r::ResponseOutputItem::FunctionCall(v) => self.emit(budget, out, |sequence_number| {
                s::StreamEvent::FunctionCallArgumentsDone(s::FunctionCallArgumentsDone {
                    sequence_number,
                    item_id: v.id.clone().expect("allocated item ID"),
                    output_index: index,
                    name: v.name.clone(),
                    arguments: v.arguments.clone(),
                    rest: Default::default(),
                })
            }),
            r::ResponseOutputItem::McpCall(v) => self.emit(budget, out, |sequence_number| {
                s::StreamEvent::McpArgumentsDone(s::McpArgumentsDone {
                    sequence_number,
                    item_id: v.id.clone(),
                    output_index: index,
                    arguments: v.arguments.clone(),
                    rest: Default::default(),
                })
            }),
            _ => Err(invalid("arguments done on non-call")),
        }
    }
    pub fn item_done(
        &mut self,
        budget: &mut Budget,
        out: &mut Vec<s::StreamEvent>,
        index: i64,
        item: r::ResponseOutputItem,
    ) -> Result<(), TransformError> {
        match &item {
            r::ResponseOutputItem::Message(v) => {
                let Some(i::OutputContent::Text(part)) = v.content.first() else {
                    return Err(invalid("stream message must preserve output-text channel"));
                };
                for (annotation_index, annotation) in part.annotations.iter().enumerate() {
                    let annotation = serde_json::to_value(annotation)?;
                    self.emit(budget, out, |sequence_number| {
                        s::StreamEvent::OutputTextAnnotationAdded(s::OutputTextAnnotationAdded {
                            sequence_number,
                            item_id: v.id.clone(),
                            output_index: index,
                            content_index: 0,
                            annotation_index: annotation_index as i64,
                            annotation,
                            rest: Default::default(),
                        })
                    })?;
                }
                self.emit(budget, out, |sequence_number| {
                    s::StreamEvent::OutputTextDone(s::OutputTextDone {
                        sequence_number,
                        item_id: v.id.clone(),
                        output_index: index,
                        content_index: 0,
                        text: part.text.clone(),
                        logprobs: Vec::new(),
                        rest: Default::default(),
                    })
                })?;
                self.emit(budget, out, |sequence_number| {
                    s::StreamEvent::ContentPartDone(s::ContentPartEvent {
                        sequence_number,
                        item_id: v.id.clone(),
                        output_index: index,
                        content_index: 0,
                        part: s::OutputContentPart::Text(part.clone()),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::Reasoning(v) => {
                let part = v
                    .content
                    .as_ref()
                    .and_then(|v| v.first())
                    .ok_or_else(|| invalid("missing reasoning content"))?;
                self.emit(budget, out, |sequence_number| {
                    s::StreamEvent::ReasoningTextDone(s::ReasoningTextDone {
                        sequence_number,
                        item_id: v.id.clone(),
                        output_index: index,
                        content_index: 0,
                        text: part.text.clone(),
                        rest: Default::default(),
                    })
                })?;
                self.emit(budget, out, |sequence_number| {
                    s::StreamEvent::ContentPartDone(s::ContentPartEvent {
                        sequence_number,
                        item_id: v.id.clone(),
                        output_index: index,
                        content_index: 0,
                        part: s::OutputContentPart::Reasoning(s::ReasoningText {
                            type_: s::ReasoningTextType::ReasoningText,
                            text: part.text.clone(),
                            rest: Default::default(),
                        }),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::McpCall(v) => {
                if matches!(
                    v.status,
                    Some(i::McpCallStatus::Completed | i::McpCallStatus::Failed)
                ) {
                    self.emit(budget, out, |sequence_number| {
                        let event = s::McpCallEvent {
                            sequence_number,
                            item_id: v.id.clone(),
                            output_index: index,
                            rest: Default::default(),
                        };
                        if v.status == Some(i::McpCallStatus::Failed) {
                            s::StreamEvent::McpFailed(event)
                        } else {
                            s::StreamEvent::McpCompleted(event)
                        }
                    })?;
                }
            }
            r::ResponseOutputItem::CustomToolCall(call) => {
                self.emit(budget, out, |sequence_number| {
                    s::StreamEvent::CustomToolInputDone(s::CustomToolInputDone {
                        sequence_number,
                        output_index: index,
                        item_id: call.id.clone().expect("allocated custom tool ID"),
                        input: call.input.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            r::ResponseOutputItem::FunctionCall(_)
            | r::ResponseOutputItem::ShellCall(_)
            | r::ResponseOutputItem::ApplyPatchCall(_)
            | r::ResponseOutputItem::ToolSearchCall(_)
            | r::ResponseOutputItem::ImageGenerationCall(_) => {}
            _ => return Err(invalid("unsupported projected output item")),
        }
        self.emit(budget, out, |sequence_number| {
            s::StreamEvent::OutputItemDone(s::OutputItemEvent {
                sequence_number,
                output_index: index,
                item,
                rest: Default::default(),
            })
        })
    }
}

pub(super) fn text(value: String, annotations: Vec<i::OutputAnnotation>) -> i::ResponseOutputText {
    i::ResponseOutputText {
        type_: i::ResponseOutputTextType::ResponseOutputText,
        text: value,
        annotations,
        logprobs: Vec::new(),
        rest: Default::default(),
    }
}
