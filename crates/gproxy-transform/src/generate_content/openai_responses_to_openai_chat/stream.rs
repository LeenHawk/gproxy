use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;
use crate::envelope::{Converter, SseFrame};

mod events;
mod terminal;
mod tools;
use events::*;
use tools::*;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
struct State {
    id: Option<String>,
    created_at: Option<u64>,
    model: Option<openai::OpenAiModelId>,
    text: Option<Item>,
    reasoning: Option<Item>,
    tools: BTreeMap<u32, Tool>,
    next_index: u32,
    usage: Option<openai::ResponseUsage>,
    finish_reason: Option<openai::ChatFinishReason>,
    sequence: u64,
    service_tier: Option<openai::ServiceTier>,
    response_rest: openai::Rest,
    started: bool,
    stopped: bool,
}

#[derive(Clone)]
struct Item {
    id: String,
    index: u32,
    text: String,
    rest: openai::Rest,
    logprobs: Vec<openai::TokenLogprob>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Custom,
}

#[derive(Clone)]
struct Tool {
    id: String,
    index: u32,
    name: String,
    arguments: String,
    kind: ToolKind,
    rest: openai::Rest,
}

impl State {
    fn chat(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if frame.data == "[DONE]" {
            return self.stop();
        }
        let chunk: openai::ChatCompletionChunk = serde_json::from_str(&frame.data)?;
        self.id = Some(chunk.id);
        self.created_at = chunk.created.or(self.created_at);
        self.model = Some(chunk.model);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        merge_rest(&mut self.response_rest, chunk.rest);
        if let Some(fingerprint) = chunk.system_fingerprint {
            self.response_rest.insert(
                "system_fingerprint".into(),
                serde_json::Value::String(fingerprint),
            );
        }
        self.usage = chunk
            .usage
            .map(usage::chat_to_responses)
            .or(self.usage.take());
        let mut output = self.ensure_start()?;
        for choice in chunk.choices {
            output.extend(self.choice(choice)?);
        }
        Ok(output)
    }

    fn choice(&mut self, choice: openai::ChatChunkChoice) -> Result<Vec<Bytes>, TransformError> {
        if choice.index != 0 {
            return Err(TransformError::unsupported(
                "Chat stream",
                "multiple choices",
            ));
        }
        if choice.delta.refusal.is_some() {
            return Err(TransformError::unsupported("Chat stream", "refusal delta"));
        }
        if choice.delta.function_call.is_some() {
            return Err(TransformError::unsupported(
                "Chat stream",
                "legacy function_call delta",
            ));
        }
        if !choice
            .logprobs
            .as_ref()
            .is_none_or(|logprobs| logprobs.refusal.is_empty())
        {
            return Err(TransformError::unsupported(
                "Chat stream",
                "refusal logprobs",
            ));
        }
        merge_rest(&mut self.response_rest, choice.rest);
        let content_logprobs = choice
            .logprobs
            .map(|logprobs| logprobs.content)
            .unwrap_or_default();
        let delta = choice.delta;
        let has_content = delta.content.is_some();
        let has_reasoning = delta.reasoning_content.is_some();
        let has_tools = delta.tool_calls.is_some();
        let mut delta_rest = delta.rest;
        if let Some(obfuscation) = delta.obfuscation {
            delta_rest.insert("obfuscation".into(), serde_json::Value::String(obfuscation));
        }
        let mut output = Vec::new();
        if let Some(text) = delta.content {
            output.extend(self.text_delta(text, delta_rest.clone(), content_logprobs)?);
        } else if !content_logprobs.is_empty() {
            return Err(TransformError::shape(
                "Chat stream",
                "content logprobs without content delta",
            ));
        }
        if let Some(reasoning) = delta.reasoning_content {
            output.extend(self.reasoning_delta(reasoning, delta_rest.clone())?);
        }
        for call in delta.tool_calls.into_iter().flatten() {
            output.extend(self.tool_delta(call)?);
        }
        if !has_content && !has_reasoning && !has_tools {
            merge_rest(&mut self.response_rest, delta_rest);
        }
        if choice.finish_reason.is_some() {
            self.finish_reason = choice.finish_reason;
        }
        Ok(output)
    }

    fn ensure_start(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.started {
            return Ok(Vec::new());
        }
        let response = self.response(openai::ResponseStatus::InProgress)?;
        self.started = true;
        Ok(vec![self.emit(
            openai::ResponseStreamEventTypeKnown::ResponseCreated,
            Some(Box::new(response)),
            None,
            None,
            None,
            None,
        )?])
    }

    fn text_delta(
        &mut self,
        delta: String,
        rest: openai::Rest,
        logprobs: Vec<openai::TokenLogprob>,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if self.text.is_none() {
            let item = Item {
                id: self.item_id("msg")?,
                index: self.allocate(),
                text: String::new(),
                rest: rest.clone(),
                logprobs: Vec::new(),
            };
            output.push(self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
                None,
                Some(Box::new(message_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::InProgress,
                ))),
                Some(item.index),
                None,
                None,
            )?);
            output.push(self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded,
                None,
                None,
                Some(item.index),
                Some(item.id.clone()),
                Some(openai::ResponseContentPart::OutputText(message_part(&item))),
            )?);
            self.text = Some(item);
        }
        let (id, index) = {
            let item = self.text.as_mut().expect("created");
            item.text.push_str(&delta);
            item.logprobs.extend(logprobs.clone());
            merge_rest(&mut item.rest, rest);
            (item.id.clone(), item.index)
        };
        output.push(self.emit_text_delta(id, index, delta, logprobs)?);
        Ok(output)
    }

    fn reasoning_delta(
        &mut self,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if self.reasoning.is_none() {
            let item = Item {
                id: self.item_id("rs")?,
                index: self.allocate(),
                text: String::new(),
                rest: rest.clone(),
                logprobs: Vec::new(),
            };
            output.push(self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
                None,
                Some(Box::new(reasoning_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::InProgress,
                ))),
                Some(item.index),
                None,
                None,
            )?);
            self.reasoning = Some(item);
        }
        let (id, index) = {
            let item = self.reasoning.as_mut().expect("created");
            item.text.push_str(&delta);
            merge_rest(&mut item.rest, rest);
            (item.id.clone(), item.index)
        };
        output.push(self.emit_delta(
            openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta,
            id,
            index,
            delta,
        )?);
        Ok(output)
    }

    fn tool_delta(
        &mut self,
        call: openai::ChatToolCallDelta,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        let chat_index = call.index;
        if !self.tools.contains_key(&chat_index) {
            let id = call.id.clone().ok_or_else(|| {
                TransformError::shape("Chat stream", "tool call id missing on first delta")
            })?;
            let kind = tool_kind(&call)?;
            let (name, rest) = tool_metadata(&call, kind)?;
            let item = Tool {
                id,
                index: self.allocate(),
                name,
                arguments: String::new(),
                kind,
                rest,
            };
            output.push(self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
                None,
                Some(Box::new(tool_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::InProgress,
                ))),
                Some(item.index),
                None,
                None,
            )?);
            self.tools.insert(chat_index, item);
        }
        let item = self.tools.get(&chat_index).expect("created");
        if call.id.as_ref().is_some_and(|id| id != &item.id) {
            return Err(TransformError::shape(
                "Chat stream",
                "tool call id changed between deltas",
            ));
        }
        let kind = tool_kind_or(&call, item.kind)?;
        if kind != item.kind {
            return Err(TransformError::shape(
                "Chat stream",
                "tool call kind changed between deltas",
            ));
        }
        let (delta, name, rest) = tool_payload(call, kind)?;
        let (id, output_index) = {
            let item = self.tools.get_mut(&chat_index).expect("created");
            if name.as_ref().is_some_and(|name| name != &item.name) {
                return Err(TransformError::shape(
                    "Chat stream",
                    "tool call name changed between deltas",
                ));
            }
            item.arguments.push_str(&delta);
            merge_rest(&mut item.rest, rest);
            (item.id.clone(), item.index)
        };
        if !delta.is_empty() {
            output.push(self.emit_delta(
                match kind {
                    ToolKind::Function => {
                        openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta
                    }
                    ToolKind::Custom => {
                        openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta
                    }
                },
                id,
                output_index,
                delta,
            )?);
        }
        Ok(output)
    }

    fn emit_delta(
        &mut self,
        type_: openai::ResponseStreamEventTypeKnown,
        item_id: String,
        output_index: u32,
        delta: String,
    ) -> Result<Bytes, TransformError> {
        let content_index = matches!(
            type_,
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta
                | openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta
                | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDelta
        );
        let mut event = event(type_);
        event.sequence_number = Some(self.next_sequence());
        event.item_id = Some(item_id);
        event.output_index = Some(output_index);
        event.content_index = content_index.then_some(0);
        event.delta = Some(delta);
        emit(event)
    }

    fn emit_text_delta(
        &mut self,
        item_id: String,
        output_index: u32,
        delta: String,
        logprobs: Vec<openai::TokenLogprob>,
    ) -> Result<Bytes, TransformError> {
        let mut event = event(openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta);
        event.sequence_number = Some(self.next_sequence());
        event.item_id = Some(item_id);
        event.output_index = Some(output_index);
        event.content_index = Some(0);
        event.delta = Some(delta);
        event.logprobs =
            (!logprobs.is_empty()).then(|| logprobs.into_iter().map(stream_logprob).collect());
        emit(event)
    }

    fn emit(
        &mut self,
        type_: openai::ResponseStreamEventTypeKnown,
        response: Option<Box<openai::ResponseObject>>,
        item: Option<Box<openai::ResponseItem>>,
        output_index: Option<u32>,
        item_id: Option<String>,
        part: Option<openai::ResponseContentPart>,
    ) -> Result<Bytes, TransformError> {
        let content_part = matches!(
            type_,
            openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded
                | openai::ResponseStreamEventTypeKnown::ResponseContentPartDone
        );
        let mut event = event(type_);
        event.sequence_number = Some(self.next_sequence());
        event.response = response;
        event.item = item;
        event.output_index = output_index;
        event.item_id = item_id;
        event.part = part;
        event.content_index = content_part.then_some(0);
        emit(event)
    }

    fn item_id(&self, prefix: &str) -> Result<String, TransformError> {
        self.id
            .as_ref()
            .map(|id| format!("{prefix}_{id}"))
            .ok_or_else(|| TransformError::shape("Chat stream", "id missing"))
    }

    fn allocate(&mut self) -> u32 {
        let value = self.next_index;
        self.next_index += 1;
        value
    }

    fn next_sequence(&mut self) -> u64 {
        let value = self.sequence;
        self.sequence += 1;
        value
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.chat(frame)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
