//! Typed aggregation state machine for Responses SSE streams: fills in
//! synthetic lifecycle events (item_added / *_done / response.completed) that
//! sparse upstreams omit, so the inbound side always sees a complete stream.

mod text;
mod tool;

use std::collections::BTreeMap;

use crate::protocol::openai::{
    Extra, KnownResponseStreamEvent as KnownEvent, ResponseItem, ResponseMessageItem,
    ResponseObject, ResponseObjectType, ResponseOutputItem, ResponseStatus, ResponseStreamEvent,
    TypedResponseItem,
};

use super::{SseDecoder, SseFrame, encode_responses_event};
use text::ResponsesTextItemState;
use tool::{ResponsesToolItemState, ResponsesToolKind};

/// Stateful normalizer for an upstream that already speaks Responses SSE.
#[derive(Default)]
pub struct ResponsesStreamNormalizer {
    decoder: SseDecoder,
    responses: ResponsesStreamState,
}

impl ResponsesStreamNormalizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(chunk) {
            self.normalize_into(frame, &mut out);
        }
        out
    }

    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish() {
            self.normalize_into(frame, &mut out);
        }
        out
    }

    fn normalize_into(&mut self, frame: SseFrame, out: &mut Vec<u8>) {
        if frame.data.trim() == "[DONE]" {
            out.extend_from_slice(frame.encode().as_bytes());
            return;
        }
        let Ok(event) = serde_json::from_str::<ResponseStreamEvent>(&frame.data) else {
            out.extend_from_slice(frame.encode().as_bytes());
            return;
        };
        for event in self.responses.push(event) {
            encode_responses_event(&event, out);
        }
    }
}

#[derive(Default)]
pub(super) struct ResponsesStreamState {
    message: ResponsesTextItemState,
    reasoning: ResponsesTextItemState,
    tools: BTreeMap<u32, ResponsesToolItemState>,
    completed: bool,
}

impl ResponsesStreamState {
    pub(super) fn push(&mut self, event: ResponseStreamEvent) -> Vec<ResponseStreamEvent> {
        let mut event = match event {
            ResponseStreamEvent::Known(known) => known,
            unknown => return vec![unknown],
        };
        let mut out = match &mut event {
            KnownEvent::ResponseOutputTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let mut out = self.finish_reasoning();
                out.extend(self.message.ensure(
                    item_id,
                    *output_index,
                    *content_index,
                    text::message_item_added,
                ));
                self.message.text.push_str(delta);
                out
            }
            KnownEvent::ResponseReasoningTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let out = self.reasoning.ensure(
                    item_id,
                    *output_index,
                    *content_index,
                    text::reasoning_item_added,
                );
                self.reasoning.text.push_str(delta);
                out
            }
            KnownEvent::ResponseFunctionCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.note_tool_input_delta(
                    ResponsesToolKind::Function,
                    *output_index,
                    item_id,
                    delta,
                );
                Vec::new()
            }
            KnownEvent::ResponseCustomToolCallInputDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.note_tool_input_delta(
                    ResponsesToolKind::Custom,
                    *output_index,
                    item_id,
                    delta,
                );
                Vec::new()
            }
            KnownEvent::ResponseFunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                output_index,
                ..
            } => {
                self.note_tool_input_done(
                    ResponsesToolKind::Function,
                    *output_index,
                    item_id,
                    arguments,
                    Some(name),
                );
                Vec::new()
            }
            KnownEvent::ResponseCustomToolCallInputDone {
                input,
                item_id,
                output_index,
                ..
            } => {
                self.note_tool_input_done(
                    ResponsesToolKind::Custom,
                    *output_index,
                    item_id,
                    input,
                    None,
                );
                Vec::new()
            }
            KnownEvent::ResponseCompleted { response, .. } => {
                let mut out = self.finish_reasoning();
                out.extend(self.finish_message());
                out.extend(self.finish_tools());
                self.patch_completed_output(response);
                self.completed = true;
                out
            }
            KnownEvent::ResponseOutputItemAdded {
                item, output_index, ..
            } => {
                self.note_item_added(item, *output_index);
                Vec::new()
            }
            KnownEvent::ResponseOutputItemDone {
                item, output_index, ..
            } => {
                self.note_item_done(item, *output_index);
                Vec::new()
            }
            KnownEvent::ResponseOutputTextDone { text, .. } => {
                self.message.note_done_text(text);
                Vec::new()
            }
            KnownEvent::ResponseReasoningTextDone { text, .. } => {
                self.reasoning.note_done_text(text);
                Vec::new()
            }
            _ => Vec::new(),
        };
        out.push(ResponseStreamEvent::Known(event));
        out
    }

    pub(super) fn finish(&mut self) -> Vec<ResponseStreamEvent> {
        if self.completed {
            return Vec::new();
        }
        let mut out = self.finish_reasoning();
        out.extend(self.finish_message());
        if !out.is_empty() {
            out.extend(self.finish_tools());
            out.push(known(KnownEvent::ResponseCompleted {
                response: Box::new(fallback_completed_response()),
                sequence_number: None,
                extra: Extra::new(),
            }));
            self.completed = true;
        }
        out
    }

    fn finish_message(&mut self) -> Vec<ResponseStreamEvent> {
        self.message.finish(text::message_done_events)
    }

    fn finish_reasoning(&mut self) -> Vec<ResponseStreamEvent> {
        self.reasoning.finish(text::reasoning_done_events)
    }

    fn note_item_added(&mut self, item: &ResponseOutputItem, output_index: u32) {
        match &item.0 {
            ResponseItem::Message(message) if message_has_type(message) => {
                self.message.note_added(message_id(message), output_index);
            }
            ResponseItem::Typed(TypedResponseItem::Reasoning { id, .. }) => {
                self.reasoning.note_added(id.as_deref(), output_index);
            }
            ResponseItem::Typed(typed @ TypedResponseItem::FunctionCall { .. }) => {
                self.note_tool_added(typed, ResponsesToolKind::Function, output_index);
            }
            ResponseItem::Typed(typed @ TypedResponseItem::CustomToolCall { .. }) => {
                self.note_tool_added(typed, ResponsesToolKind::Custom, output_index);
            }
            _ => {}
        }
    }

    fn note_item_done(&mut self, item: &ResponseOutputItem, output_index: u32) {
        match &item.0 {
            ResponseItem::Message(message) if message_has_type(message) => {
                self.message
                    .note_item_done(message_id(message), output_index);
            }
            ResponseItem::Typed(TypedResponseItem::Reasoning { id, .. }) => {
                self.reasoning.note_item_done(id.as_deref(), output_index);
            }
            ResponseItem::Typed(typed @ TypedResponseItem::FunctionCall { .. }) => {
                self.note_tool_item_done(typed, ResponsesToolKind::Function, output_index);
            }
            ResponseItem::Typed(typed @ TypedResponseItem::CustomToolCall { .. }) => {
                self.note_tool_item_done(typed, ResponsesToolKind::Custom, output_index);
            }
            _ => {}
        }
    }

    fn note_tool_added(&mut self, item: &TypedResponseItem, kind: ResponsesToolKind, index: u32) {
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.note_item(item);
    }

    fn note_tool_item_done(
        &mut self,
        item: &TypedResponseItem,
        kind: ResponsesToolKind,
        index: u32,
    ) {
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.item_done = true;
        state.note_item(item);
    }

    fn note_tool_input_delta(
        &mut self,
        kind: ResponsesToolKind,
        index: u32,
        item_id: &mut String,
        delta: &str,
    ) {
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.note_event_item_id(item_id);
        state.rewrite_event_item_id(item_id);
        state.input.push_str(delta);
    }

    fn note_tool_input_done(
        &mut self,
        kind: ResponsesToolKind,
        index: u32,
        item_id: &mut String,
        input: &str,
        name: Option<&str>,
    ) {
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.note_event_item_id(item_id);
        state.rewrite_event_item_id(item_id);
        input.clone_into(&mut state.input);
        if let Some(name) = name {
            state.name.get_or_insert_with(|| name.to_owned());
        }
        state.input_done = true;
    }

    fn finish_tools(&mut self) -> Vec<ResponseStreamEvent> {
        let mut out = Vec::new();
        for state in self.tools.values_mut() {
            if !state.can_finish() {
                continue;
            }
            if !state.input_done {
                out.push(state.input_done_event());
                state.input_done = true;
            }
            if !state.item_done {
                out.push(state.item_done_event());
                state.item_done = true;
            }
        }
        out
    }

    fn patch_completed_output(&self, response: &mut ResponseObject) {
        if !response.output.is_empty() {
            return;
        }
        let output = self.completed_output_items();
        if !output.is_empty() {
            response.output = output;
        }
    }

    fn completed_output_items(&self) -> Vec<ResponseOutputItem> {
        use crate::protocol::openai::ResponseItemLifecycleStatus::Completed;
        let mut output = Vec::new();
        if self.reasoning.started {
            output.push(text::reasoning_item(&self.reasoning, Completed));
        }
        if self.message.started {
            output.push(text::message_item(&self.message, Completed));
        }
        output.extend(
            self.tools
                .values()
                .filter(|state| state.can_finish())
                .map(ResponsesToolItemState::completed_item),
        );
        output
    }
}

fn known(event: KnownEvent) -> ResponseStreamEvent {
    ResponseStreamEvent::Known(event)
}

fn message_has_type(message: &ResponseMessageItem) -> bool {
    match message {
        ResponseMessageItem::Output(_) => true,
        ResponseMessageItem::Input(input) => input.type_.is_some(),
        ResponseMessageItem::EasyInput(easy) => easy.type_.is_some(),
    }
}

fn message_id(message: &ResponseMessageItem) -> Option<&str> {
    match message {
        ResponseMessageItem::Output(output) => Some(&output.id),
        ResponseMessageItem::Input(input) => input.id.as_deref(),
        ResponseMessageItem::EasyInput(_) => None,
    }
}

/// Fallback `response.completed` payload for streams that never sent one.
fn fallback_completed_response() -> ResponseObject {
    ResponseObject {
        id: "resp_0".to_owned(),
        created_at: 0,
        background: None,
        completed_at: Some(0),
        conversation: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: None,
        moderation: None,
        multi_agent: None,
        object: ResponseObjectType::Response,
        output: Vec::new(),
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        status: Some(ResponseStatus::Completed),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: None,
        user: None,
        extra: Extra::new(),
    }
}
