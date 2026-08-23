use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;
use crate::envelope::{Converter, SseFrame};

mod items;
mod native;
mod wire;

use wire::{coordinates, empty_delta, merge_rest, required};

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
struct State {
    id: Option<String>,
    created_at: Option<u64>,
    model: Option<openai::OpenAiModelId>,
    service_tier: Option<openai::ServiceTier>,
    tools: BTreeMap<String, Tool>,
    next_tool: u32,
    text: String,
    reasoning: String,
    refusal: String,
    started: bool,
    stopped: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Custom,
}

struct Tool {
    index: u32,
    output_index: u32,
    kind: ToolKind,
    call_id: String,
    name: String,
    data: String,
}

struct ToolStart {
    source_id: String,
    call_id: String,
    output_index: u32,
    name: String,
    kind: ToolKind,
    rest: openai::Rest,
    event_rest: openai::Rest,
}

impl State {
    fn event(&mut self, event: openai::ResponseStreamEvent) -> Result<Vec<Bytes>, TransformError> {
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Err(TransformError::unsupported(
                "Responses stream event",
                "unknown event",
            ));
        };
        if let Some(response) = event.response.as_ref() {
            self.update_response(response);
        }
        match event.type_ {
            openai::ResponseStreamEventTypeKnown::ResponseCreated
            | openai::ResponseStreamEventTypeKnown::ResponseInProgress => self.start(*event),
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta => {
                self.text_delta(*event)
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta => {
                self.reasoning_delta(*event, false)
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDelta => {
                self.reasoning_delta(*event, true)
            }
            openai::ResponseStreamEventTypeKnown::ResponseRefusalDelta => {
                self.refusal_delta(*event)
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded
            | openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone => {
                let event = *event;
                let output_index = required(event.output_index, "output_index")?;
                let item = required(event.item.map(|item| *item), "item")?;
                self.complete_item(item, output_index, event.rest)
            }
            openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded
            | openai::ResponseStreamEventTypeKnown::ResponseContentPartDone => {
                let event = *event;
                let item_id = required(event.item_id, "item_id")?;
                let output_index = required(event.output_index, "output_index")?;
                let content_index = required(event.content_index, "content_index")?;
                let part = required(event.part, "part")?;
                self.complete_part(part, item_id, output_index, content_index, event.rest)
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDone => {
                let event = *event;
                coordinates(&event, true)?;
                self.finish_text(
                    required(event.text, "text")?,
                    Default::default(),
                    event.rest,
                )
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDone => {
                let event = *event;
                coordinates(&event, true)?;
                self.finish_reasoning(
                    required(event.text, "text")?,
                    Default::default(),
                    event.rest,
                )
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDone => {
                let event = *event;
                coordinates(&event, false)?;
                required(event.summary_index, "summary_index")?;
                self.finish_reasoning(
                    required(event.text, "text")?,
                    Default::default(),
                    event.rest,
                )
            }
            openai::ResponseStreamEventTypeKnown::ResponseRefusalDone => {
                let event = *event;
                coordinates(&event, true)?;
                self.finish_refusal(
                    required(event.refusal, "refusal")?,
                    Default::default(),
                    event.rest,
                )
            }
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta => {
                self.tool_delta(*event, ToolKind::Function)
            }
            openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta => {
                self.tool_delta(*event, ToolKind::Custom)
            }
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone => {
                let event = *event;
                let name = required(event.name.clone(), "name")?;
                self.tool_done(event, ToolKind::Function, Some(name), |event| {
                    event.arguments.take()
                })
            }
            openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone => {
                let event = *event;
                self.tool_done(event, ToolKind::Custom, None, |event| event.input.take())
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryPartAdded
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryPartDone => {
                let event = *event;
                required(event.item_id, "item_id")?;
                required(event.output_index, "output_index")?;
                required(event.summary_index, "summary_index")?;
                let part = required(event.reasoning_part, "part")?;
                self.finish_reasoning(part.text, part.rest, event.rest)
            }
            openai::ResponseStreamEventTypeKnown::ResponseCompleted => {
                self.terminal(*event, openai::ResponseStatus::Completed)
            }
            openai::ResponseStreamEventTypeKnown::ResponseIncomplete => {
                self.terminal(*event, openai::ResponseStatus::Incomplete)
            }
            openai::ResponseStreamEventTypeKnown::ResponseFailed
            | openai::ResponseStreamEventTypeKnown::Error => Err(TransformError::unsupported(
                "Responses stream",
                event.type_.as_str(),
            )),
            openai::ResponseStreamEventTypeKnown::ResponseQueued
            | openai::ResponseStreamEventTypeKnown::ResponseOutputTextAnnotationAdded
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
            | openai::ResponseStreamEventTypeKnown::ResponseMcpListToolsFailed => Err(
                TransformError::unsupported("Responses stream", event.type_.as_str()),
            ),
        }
    }

    fn start(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let response = event
            .response
            .ok_or_else(|| TransformError::shape("Responses stream", "response missing"))?;
        if response
            .status
            .as_ref()
            .is_some_and(|status| status != &openai::ResponseStatus::InProgress)
        {
            return Err(TransformError::shape(
                "Responses stream",
                "start event response status is not in_progress",
            ));
        }
        self.update_response(&response);
        let mut rest = response.rest.clone();
        merge_rest(&mut rest, event.rest);
        if self.started {
            return if rest.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![self.chunk(empty_delta(), None, None, rest)?])
            };
        }
        self.started = true;
        Ok(vec![self.chunk(
            openai::ChatDelta {
                role: Some(openai::ChatDeltaRole::Assistant),
                ..empty_delta()
            },
            None,
            None,
            rest,
        )?])
    }

    fn text_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        coordinates(&event, true)?;
        if event.logprobs.is_some() {
            return Err(TransformError::unsupported(
                "Responses stream",
                "output text delta logprobs",
            ));
        }
        let delta = required(event.delta, "delta")?;
        self.text.push_str(&delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                content: Some(delta),
                ..empty_delta()
            },
            None,
            None,
            event.rest,
        )?])
    }

    fn reasoning_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
        summary: bool,
    ) -> Result<Vec<Bytes>, TransformError> {
        coordinates(&event, !summary)?;
        if summary {
            required(event.summary_index, "summary_index")?;
        }
        let delta = required(event.delta, "delta")?;
        self.reasoning.push_str(&delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                reasoning_content: Some(delta),
                ..empty_delta()
            },
            None,
            None,
            event.rest,
        )?])
    }

    fn refusal_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        coordinates(&event, true)?;
        let delta = required(event.delta, "delta")?;
        self.refusal.push_str(&delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                refusal: Some(delta),
                ..empty_delta()
            },
            None,
            None,
            event.rest,
        )?])
    }

    fn tool_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
        kind: ToolKind,
    ) -> Result<Vec<Bytes>, TransformError> {
        let (id, output_index) = coordinates(&event, false)?;
        let delta = required(event.delta, "delta")?;
        let tool = self.tools.get_mut(&id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool delta before output item")
        })?;
        if tool.kind != kind || tool.output_index != output_index {
            return Err(TransformError::shape(
                "Responses stream",
                "tool delta does not match its output item",
            ));
        }
        tool.data.push_str(&delta);
        let index = tool.index;
        Ok(vec![self.tool_chunk(index, kind, delta, event.rest)?])
    }

    fn tool_done<F>(
        &mut self,
        mut event: openai::KnownResponseStreamEvent,
        kind: ToolKind,
        name: Option<String>,
        value: F,
    ) -> Result<Vec<Bytes>, TransformError>
    where
        F: FnOnce(&mut openai::KnownResponseStreamEvent) -> Option<String>,
    {
        let (id, output_index) = coordinates(&event, false)?;
        if let Some(name) = name {
            let tool = self.tools.get(&id).ok_or_else(|| {
                TransformError::shape("Responses stream", "tool done before output item")
            })?;
            if tool.name != name {
                return Err(TransformError::shape(
                    "Responses stream",
                    "tool done name does not match its output item",
                ));
            }
        }
        let full = required(value(&mut event), "tool input")?;
        self.finish_tool(&id, output_index, kind, full, event.rest)
    }

    fn terminal(
        &mut self,
        event: openai::KnownResponseStreamEvent,
        expected: openai::ResponseStatus,
    ) -> Result<Vec<Bytes>, TransformError> {
        let response = event.response.ok_or_else(|| {
            TransformError::shape("Responses stream", "terminal response missing")
        })?;
        if response
            .status
            .as_ref()
            .is_some_and(|status| status != &expected)
        {
            return Err(TransformError::shape(
                "Responses stream",
                "terminal event type does not match response status",
            ));
        }
        self.update_response(&response);
        let mut output = Vec::new();
        for (index, item) in response.output.iter().cloned().enumerate() {
            output.extend(self.complete_item(item, index as u32, Default::default())?);
        }
        let finish = match expected {
            openai::ResponseStatus::Completed if self.tools.is_empty() => {
                openai::ChatFinishReason::Stop
            }
            openai::ResponseStatus::Completed => openai::ChatFinishReason::ToolCalls,
            openai::ResponseStatus::Incomplete
                if matches!(
                    response
                        .incomplete_details
                        .as_ref()
                        .and_then(|value| value.reason.as_ref()),
                    Some(openai::IncompleteReason::ContentFilter)
                ) =>
            {
                openai::ChatFinishReason::ContentFilter
            }
            openai::ResponseStatus::Incomplete => openai::ChatFinishReason::Length,
            _ => {
                return Err(TransformError::shape(
                    "Responses stream",
                    "unsupported successful terminal status",
                ));
            }
        };
        let mut rest = response.rest.clone();
        merge_rest(&mut rest, event.rest);
        self.stopped = true;
        output.push(self.chunk(
            empty_delta(),
            Some(finish),
            response.usage.clone().map(usage::responses_to_chat),
            rest,
        )?);
        output.push(SseFrame::encode(None, "[DONE]"));
        Ok(output)
    }

    fn update_response(&mut self, response: &openai::ResponseObject) {
        self.id = Some(response.id.clone());
        self.created_at = response.created_at.or(self.created_at);
        self.model = response.model.clone().or(self.model.take());
        self.service_tier = response.service_tier.clone().or(self.service_tier.take());
    }

    fn chunk(
        &self,
        delta: openai::ChatDelta,
        finish_reason: Option<openai::ChatFinishReason>,
        usage: Option<openai::CompletionUsage>,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        SseFrame::typed(
            None,
            &openai::ChatCompletionChunk {
                id: self
                    .id
                    .clone()
                    .ok_or_else(|| TransformError::shape("Responses stream", "id missing"))?,
                choices: vec![openai::ChatChunkChoice {
                    index: 0,
                    delta,
                    finish_reason,
                    logprobs: None,
                    rest: Default::default(),
                }],
                created: self.created_at,
                model: self
                    .model
                    .clone()
                    .ok_or_else(|| TransformError::shape("Responses stream", "model missing"))?,
                object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
                service_tier: self.service_tier.clone(),
                system_fingerprint: None,
                usage,
                rest,
            },
        )
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.event(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
