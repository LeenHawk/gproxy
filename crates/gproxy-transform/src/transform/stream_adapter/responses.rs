mod text;
mod tool;

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{ContentGenerationKind, SseDecoder, SseFrame, encode_frame};
use text::{
    ResponsesTextItemState, message_item, message_item_added, reasoning_item, reasoning_item_added,
};
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
        let Ok(event) = serde_json::from_str::<Value>(&frame.data) else {
            out.extend_from_slice(frame.encode().as_bytes());
            return;
        };
        for event in self.responses.push(event) {
            out.extend_from_slice(
                encode_frame(ContentGenerationKind::OpenAiResponses, &event).as_bytes(),
            );
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
    pub(super) fn push(&mut self, mut event: Value) -> Vec<Value> {
        match event.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                let mut out = self.finish_reasoning();
                out.extend(self.message.ensure(&event, "msg_0", message_item_added));
                self.message.push_delta(&event);
                out.push(event);
                out
            }
            Some("response.reasoning_text.delta") => {
                let mut out = self
                    .reasoning
                    .ensure(&event, "reasoning_0", reasoning_item_added);
                self.reasoning.push_delta(&event);
                out.push(event);
                out
            }
            Some("response.function_call_arguments.delta") => {
                self.note_tool_input_delta(&mut event, ResponsesToolKind::Function);
                vec![event]
            }
            Some("response.custom_tool_call_input.delta") => {
                self.note_tool_input_delta(&mut event, ResponsesToolKind::Custom);
                vec![event]
            }
            Some("response.function_call_arguments.done") => {
                self.note_tool_input_done(&mut event, ResponsesToolKind::Function);
                vec![event]
            }
            Some("response.custom_tool_call_input.done") => {
                self.note_tool_input_done(&mut event, ResponsesToolKind::Custom);
                vec![event]
            }
            Some("response.completed") => {
                let mut out = self.finish_reasoning();
                out.extend(self.finish_message());
                out.extend(self.finish_tools());
                self.patch_completed_output(&mut event);
                self.completed = true;
                out.push(event);
                out
            }
            Some("response.output_item.added") => {
                self.note_item_added(&event);
                vec![event]
            }
            Some("response.output_item.done") => {
                self.note_item_done(&event);
                vec![event]
            }
            Some("response.output_text.done") => {
                self.message.note_done_text(&event);
                vec![event]
            }
            Some("response.reasoning_text.done") => {
                self.reasoning.note_done_text(&event);
                vec![event]
            }
            _ => vec![event],
        }
    }

    pub(super) fn finish(&mut self) -> Vec<Value> {
        if self.completed {
            return Vec::new();
        }
        let mut out = self.finish_reasoning();
        out.extend(self.finish_message());
        if !out.is_empty() {
            out.extend(self.finish_tools());
            out.push(json!({
                "type": "response.completed",
                "response": {"id":"resp_0","object":"response","created_at":0,
                    "completed_at":0,"status":"completed","output":[]},
            }));
            self.completed = true;
        }
        out
    }

    fn finish_message(&mut self) -> Vec<Value> {
        self.message.finish(|state| {
            vec![
                json!({"type":"response.output_text.done","output_index":state.output_index(),
                "item_id":state.id(),"content_index":state.content_index(),"text":state.text}),
                json!({"type":"response.content_part.done","output_index":state.output_index(),
                "item_id":state.id(),"content_index":state.content_index(),
                "part":{"type":"output_text","text":state.text,"annotations":[]}}),
                json!({"type":"response.output_item.done","output_index":state.output_index(),
                "item":message_item(state,"completed")}),
            ]
        })
    }

    fn finish_reasoning(&mut self) -> Vec<Value> {
        self.reasoning.finish(|state| {
            vec![
                json!({"type":"response.reasoning_text.done","output_index":state.output_index(),
                "item_id":state.id(),"content_index":state.content_index(),"text":state.text}),
                json!({"type":"response.output_item.done","output_index":state.output_index(),
                "item":reasoning_item(state,"completed")}),
            ]
        })
    }

    fn note_item_added(&mut self, event: &Value) {
        match item_type(event) {
            Some("message") => self.message.note_added(event),
            Some("reasoning") => self.reasoning.note_added(event),
            Some("function_call") => self.note_tool_added(event, ResponsesToolKind::Function),
            Some("custom_tool_call") => self.note_tool_added(event, ResponsesToolKind::Custom),
            _ => {}
        }
    }

    fn note_item_done(&mut self, event: &Value) {
        match item_type(event) {
            Some("message") => self.message.note_item_done(event),
            Some("reasoning") => self.reasoning.note_item_done(event),
            Some("function_call") => self.note_tool_item_done(event, ResponsesToolKind::Function),
            Some("custom_tool_call") => self.note_tool_item_done(event, ResponsesToolKind::Custom),
            _ => {}
        }
    }

    fn note_tool_added(&mut self, event: &Value, kind: ResponsesToolKind) {
        let Some(index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        if let Some(item) = event.get("item") {
            state.note_item(item);
        }
    }

    fn note_tool_item_done(&mut self, event: &Value, kind: ResponsesToolKind) {
        let Some(index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.item_done = true;
        if let Some(item) = event.get("item") {
            state.note_item(item);
        }
    }

    fn note_tool_input_delta(&mut self, event: &mut Value, kind: ResponsesToolKind) {
        let Some(index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.note_event_item_id(event);
        if let Some(id) = state.item_id.as_deref() {
            event["item_id"] = Value::String(id.into());
        }
        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
            state.input.push_str(delta);
        }
    }

    fn note_tool_input_done(&mut self, event: &mut Value, kind: ResponsesToolKind) {
        let Some(index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(index).or_default();
        state.note_kind(kind, index);
        state.note_event_item_id(event);
        if let Some(id) = state.item_id.as_deref() {
            event["item_id"] = Value::String(id.into());
        }
        let field = match kind {
            ResponsesToolKind::Function => "arguments",
            ResponsesToolKind::Custom => "input",
        };
        if let Some(done) = event.get(field).and_then(Value::as_str) {
            state.input = done.into();
        }
        if matches!(kind, ResponsesToolKind::Function)
            && let Some(name) = event.get("name").and_then(Value::as_str)
        {
            state.name.get_or_insert_with(|| name.into());
        }
        state.input_done = true;
    }

    fn finish_tools(&mut self) -> Vec<Value> {
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

    fn patch_completed_output(&self, event: &mut Value) {
        let Some(response) = event.get_mut("response").and_then(Value::as_object_mut) else {
            return;
        };
        if !response
            .get("output")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            return;
        }
        let output = self.completed_output_items();
        if !output.is_empty() {
            response.insert("output".into(), Value::Array(output));
        }
    }

    fn completed_output_items(&self) -> Vec<Value> {
        let mut output = Vec::new();
        if self.reasoning.started {
            output.push(reasoning_item(&self.reasoning, "completed"));
        }
        if self.message.started {
            output.push(message_item(&self.message, "completed"));
        }
        output.extend(
            self.tools
                .values()
                .filter(|state| state.can_finish())
                .map(|state| state.item("completed")),
        );
        output
    }
}

fn item_type(event: &Value) -> Option<&str> {
    event.get("item")?.get("type")?.as_str()
}

fn event_output_index(event: &Value) -> Option<u32> {
    event.get("output_index")?.as_u64()?.try_into().ok()
}
