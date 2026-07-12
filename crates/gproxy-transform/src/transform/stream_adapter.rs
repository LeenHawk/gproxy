//! Runtime SSE adapter for cross-protocol streaming: decode upstream frames,
//! convert each event through the resolved (reverse) pair, re-encode in the
//! inbound wire format. Sync core — shared by the native stream wrapper
//! (`pipeline/stream.rs`) and the buffered path (wasm / buffered attempts).
//!
//! Pair `stream_event` fns are 1:1 and stateless; any future cross-event
//! aggregation (block indexes, tool-call identity, final usage) lives HERE
//! (see transform/README.md).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::common::sse::{SseDecoder, SseFrame};
use super::{TransformContext, TransformPair, dispatch};
use crate::protocol::ContentGenerationKind;

pub struct SseTransformer {
    decoder: SseDecoder,
    /// Reverse pair: upstream kind → inbound kind.
    pair: TransformPair,
    ctx: TransformContext,
    inbound: ContentGenerationKind,
    responses: Option<ResponsesStreamState>,
    skipped: u64,
}

/// Stateful normalizer for an upstream that already speaks Responses SSE.
///
/// Unlike [`SseTransformer`], this does not perform a cross-protocol dispatch.
/// It preserves the upstream events while completing the Responses event
/// ladder and backfilling an empty `response.completed.response.output` from
/// the output items observed earlier in the stream.
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

impl SseTransformer {
    pub fn new(pair: TransformPair, ctx: TransformContext, inbound: ContentGenerationKind) -> Self {
        Self {
            decoder: SseDecoder::new(),
            pair,
            ctx,
            inbound,
            responses: matches!(
                inbound,
                ContentGenerationKind::OpenAiResponses
                    | ContentGenerationKind::OpenAiResponsesWebSocket
            )
            .then(ResponsesStreamState::default),
            skipped: 0,
        }
    }

    /// Feed one upstream chunk; returns encoded inbound bytes (possibly empty).
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(chunk) {
            self.convert_into(frame, &mut out);
        }
        out
    }

    /// Flush the trailing frame and emit the inbound terminator.
    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish() {
            self.convert_into(frame, &mut out);
        }
        if let Some(responses) = self.responses.as_mut() {
            for event in responses.finish() {
                out.extend_from_slice(encode_frame(self.inbound, &event).as_bytes());
            }
        }
        if self.inbound == ContentGenerationKind::OpenAiChatCompletions {
            out.extend_from_slice(b"data: [DONE]\n\n");
        }
        if self.skipped > 0 {
            tracing::warn!(
                skipped = self.skipped,
                "stream transform skipped unconvertible frames"
            );
        }
        out
    }

    fn convert_into(&mut self, frame: SseFrame, out: &mut Vec<u8>) {
        // upstream openai-chat terminator — represented by finish() inbound-side
        if frame.data.trim() == "[DONE]" {
            return;
        }
        let event: Value = match serde_json::from_str(&frame.data) {
            Ok(v) => v,
            Err(_) => {
                self.skipped += 1;
                return;
            }
        };
        match dispatch::stream_event_value(self.pair, &self.ctx, event) {
            Ok(converted) => {
                let events = if let Some(responses) = self.responses.as_mut() {
                    responses.push(converted)
                } else {
                    vec![converted]
                };
                for event in events {
                    out.extend_from_slice(encode_frame(self.inbound, &event).as_bytes());
                }
            }
            Err(_) => {
                self.skipped += 1;
            }
        }
    }
}

#[derive(Default)]
struct ResponsesStreamState {
    message: ResponsesTextItemState,
    reasoning: ResponsesTextItemState,
    tools: BTreeMap<u32, ResponsesToolItemState>,
    completed: bool,
}

#[derive(Default)]
struct ResponsesTextItemState {
    started: bool,
    done: bool,
    id: Option<String>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    text: String,
}

#[derive(Default)]
struct ResponsesToolItemState {
    kind: Option<ResponsesToolKind>,
    item_id: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    output_index: Option<u32>,
    input: String,
    input_done: bool,
    item_done: bool,
}

#[derive(Clone, Copy)]
enum ResponsesToolKind {
    Function,
    Custom,
}

impl ResponsesStreamState {
    fn push(&mut self, mut event: Value) -> Vec<Value> {
        match event.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                let mut out = self.finish_reasoning();
                out.extend(self.ensure_message(&event));
                self.message.push_delta(&event);
                out.push(event);
                out
            }
            Some("response.reasoning_text.delta") => {
                let mut out = self.ensure_reasoning(&event);
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

    fn finish(&mut self) -> Vec<Value> {
        if self.completed {
            return Vec::new();
        }
        let mut out = self.finish_reasoning();
        out.extend(self.finish_message());
        if !out.is_empty() {
            out.extend(self.finish_tools());
            out.push(json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_0",
                    "object": "response",
                    "created_at": 0,
                    "completed_at": 0,
                    "status": "completed",
                    "output": [],
                },
            }));
            self.completed = true;
        }
        out
    }

    fn ensure_message(&mut self, event: &Value) -> Vec<Value> {
        self.message.ensure(event, "msg_0", message_item_added)
    }

    fn ensure_reasoning(&mut self, event: &Value) -> Vec<Value> {
        self.reasoning
            .ensure(event, "reasoning_0", reasoning_item_added)
    }

    fn finish_message(&mut self) -> Vec<Value> {
        self.message.finish(|state| {
            vec![
                json!({
                    "type": "response.output_text.done",
                    "output_index": state.output_index(),
                    "item_id": state.id(),
                    "content_index": state.content_index(),
                    "text": state.text,
                }),
                json!({
                    "type": "response.content_part.done",
                    "output_index": state.output_index(),
                    "item_id": state.id(),
                    "content_index": state.content_index(),
                    "part": { "type": "output_text", "text": state.text, "annotations": [] },
                }),
                json!({
                    "type": "response.output_item.done",
                    "output_index": state.output_index(),
                    "item": message_item(state, "completed"),
                }),
            ]
        })
    }

    fn finish_reasoning(&mut self) -> Vec<Value> {
        self.reasoning.finish(|state| {
            vec![
                json!({
                    "type": "response.reasoning_text.done",
                    "output_index": state.output_index(),
                    "item_id": state.id(),
                    "content_index": state.content_index(),
                    "text": state.text,
                }),
                json!({
                    "type": "response.output_item.done",
                    "output_index": state.output_index(),
                    "item": reasoning_item(state, "completed"),
                }),
            ]
        })
    }

    fn note_item_added(&mut self, event: &Value) {
        let Some(item_type) = event
            .get("item")
            .and_then(|item| item.get("type"))
            .and_then(Value::as_str)
        else {
            return;
        };
        match item_type {
            "message" => self.message.note_added(event),
            "reasoning" => self.reasoning.note_added(event),
            "function_call" => self.note_tool_added(event, ResponsesToolKind::Function),
            "custom_tool_call" => self.note_tool_added(event, ResponsesToolKind::Custom),
            _ => {}
        }
    }

    fn note_item_done(&mut self, event: &Value) {
        let Some(item_type) = event
            .get("item")
            .and_then(|item| item.get("type"))
            .and_then(Value::as_str)
        else {
            return;
        };
        match item_type {
            "message" => self.message.note_item_done(event),
            "reasoning" => self.reasoning.note_item_done(event),
            "function_call" => {
                self.note_tool_item_done(event, ResponsesToolKind::Function);
            }
            "custom_tool_call" => {
                self.note_tool_item_done(event, ResponsesToolKind::Custom);
            }
            _ => {}
        }
    }

    fn note_tool_added(&mut self, event: &Value, kind: ResponsesToolKind) {
        let Some(output_index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(output_index).or_default();
        state.kind.get_or_insert(kind);
        state.output_index.get_or_insert(output_index);
        if let Some(item) = event.get("item") {
            state.note_item(item);
        }
    }

    fn note_tool_item_done(&mut self, event: &Value, kind: ResponsesToolKind) {
        let Some(output_index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(output_index).or_default();
        state.kind.get_or_insert(kind);
        state.output_index.get_or_insert(output_index);
        state.item_done = true;
        if let Some(item) = event.get("item") {
            state.note_item(item);
        }
    }

    fn note_tool_input_delta(&mut self, event: &mut Value, kind: ResponsesToolKind) {
        let Some(output_index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(output_index).or_default();
        state.kind.get_or_insert(kind);
        state.output_index.get_or_insert(output_index);
        state.note_event_item_id(event);
        if let Some(item_id) = state.item_id.as_deref() {
            event["item_id"] = Value::String(item_id.to_owned());
        }
        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
            state.input.push_str(delta);
        }
    }

    fn note_tool_input_done(&mut self, event: &mut Value, kind: ResponsesToolKind) {
        let Some(output_index) = event_output_index(event) else {
            return;
        };
        let state = self.tools.entry(output_index).or_default();
        state.kind.get_or_insert(kind);
        state.output_index.get_or_insert(output_index);
        state.note_event_item_id(event);
        if let Some(item_id) = state.item_id.as_deref() {
            event["item_id"] = Value::String(item_id.to_owned());
        }
        let field = match kind {
            ResponsesToolKind::Function => "arguments",
            ResponsesToolKind::Custom => "input",
        };
        if let Some(done) = event.get(field).and_then(Value::as_str) {
            state.input.clear();
            state.input.push_str(done);
        }
        if matches!(kind, ResponsesToolKind::Function)
            && let Some(name) = event.get("name").and_then(Value::as_str)
        {
            state.name.get_or_insert_with(|| name.to_owned());
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
        let Some(response) = event.get_mut("response") else {
            return;
        };
        let Some(response_obj) = response.as_object_mut() else {
            return;
        };
        let output_is_empty = response_obj
            .get("output")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty);
        if !output_is_empty {
            return;
        }

        let output = self.completed_output_items();
        if !output.is_empty() {
            response_obj.insert("output".to_owned(), Value::Array(output));
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
        for state in self.tools.values() {
            if state.can_finish() {
                output.push(state.item("completed"));
            }
        }
        output
    }
}

impl ResponsesTextItemState {
    fn ensure(
        &mut self,
        event: &Value,
        fallback_id: &'static str,
        build: impl FnOnce(&Self) -> Value,
    ) -> Vec<Value> {
        self.note_delta_identity(event, fallback_id);
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![build(self)]
    }

    fn push_delta(&mut self, event: &Value) {
        self.note_delta_identity(event, "item_0");
        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
            self.text.push_str(delta);
        }
    }

    fn finish(&mut self, build: impl FnOnce(&Self) -> Vec<Value>) -> Vec<Value> {
        if !self.started || self.done {
            return Vec::new();
        }
        self.done = true;
        build(self)
    }

    fn note_delta_identity(&mut self, event: &Value, fallback_id: &'static str) {
        if self.id.is_none() {
            self.id = event
                .get("item_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some(fallback_id.to_owned()));
        }
        if self.output_index.is_none() {
            self.output_index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .or(Some(0));
        }
        if self.content_index.is_none() {
            self.content_index = event
                .get("content_index")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .or(Some(0));
        }
    }

    fn note_added(&mut self, event: &Value) {
        self.started = true;
        self.note_item_identity(event);
    }

    fn note_item_done(&mut self, event: &Value) {
        self.done = true;
        self.note_item_identity(event);
    }

    fn note_done_text(&mut self, event: &Value) {
        self.done = true;
        if let Some(text) = event.get("text").and_then(Value::as_str) {
            self.text.clear();
            self.text.push_str(text);
        }
    }

    fn note_item_identity(&mut self, event: &Value) {
        if self.id.is_none() {
            self.id = event
                .get("item")
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if self.output_index.is_none() {
            self.output_index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
        }
    }

    fn id(&self) -> &str {
        self.id.as_deref().unwrap_or("item_0")
    }

    fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }

    fn content_index(&self) -> u32 {
        self.content_index.unwrap_or(0)
    }
}

impl ResponsesToolItemState {
    fn note_item(&mut self, item: &Value) {
        if self.item_id.is_none() {
            self.item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
        }
        if self.call_id.is_none() {
            self.call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if self.name.is_none() {
            self.name = item.get("name").and_then(Value::as_str).map(str::to_owned);
        }
        if self.input.is_empty() {
            let field = match self.kind {
                Some(ResponsesToolKind::Function) => "arguments",
                Some(ResponsesToolKind::Custom) => "input",
                None => return,
            };
            if let Some(input) = item.get(field).and_then(Value::as_str) {
                self.input.push_str(input);
            }
        }
    }

    fn note_event_item_id(&mut self, event: &Value) {
        if self.item_id.is_none() {
            self.item_id = event
                .get("item_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }

    fn can_finish(&self) -> bool {
        self.kind.is_some()
            && self.item_id.is_some()
            && self.call_id.is_some()
            && self.name.is_some()
    }

    fn input_done_event(&self) -> Value {
        match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => json!({
                "type": "response.function_call_arguments.done",
                "output_index": self.output_index(),
                "item_id": self.item_id(),
                "name": self.name(),
                "arguments": self.input,
            }),
            ResponsesToolKind::Custom => json!({
                "type": "response.custom_tool_call_input.done",
                "output_index": self.output_index(),
                "item_id": self.item_id(),
                "input": self.input,
            }),
        }
    }

    fn item_done_event(&self) -> Value {
        json!({
            "type": "response.output_item.done",
            "output_index": self.output_index(),
            "item": self.item("completed"),
        })
    }

    fn item(&self, status: &str) -> Value {
        match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => json!({
                "id": self.item_id(),
                "type": "function_call",
                "status": status,
                "call_id": self.call_id(),
                "name": self.name(),
                "arguments": self.input,
            }),
            ResponsesToolKind::Custom => json!({
                "id": self.item_id(),
                "type": "custom_tool_call",
                "call_id": self.call_id(),
                "name": self.name(),
                "input": self.input,
            }),
        }
    }

    fn item_id(&self) -> &str {
        self.item_id.as_deref().unwrap_or("item_0")
    }

    fn call_id(&self) -> &str {
        self.call_id.as_deref().unwrap_or("call_0")
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }
}

fn event_output_index(event: &Value) -> Option<u32> {
    event
        .get("output_index")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

fn message_item_added(state: &ResponsesTextItemState) -> Value {
    json!({
        "type": "response.output_item.added",
        "output_index": state.output_index(),
        "item": message_item(state, "in_progress"),
    })
}

fn reasoning_item_added(state: &ResponsesTextItemState) -> Value {
    json!({
        "type": "response.output_item.added",
        "output_index": state.output_index(),
        "item": reasoning_item(state, "in_progress"),
    })
}

fn message_item(state: &ResponsesTextItemState, status: &str) -> Value {
    json!({
        "id": state.id(),
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [{ "type": "output_text", "text": state.text, "annotations": [] }],
    })
}

fn reasoning_item(state: &ResponsesTextItemState, status: &str) -> Value {
    json!({
        "id": state.id(),
        "type": "reasoning",
        "status": status,
        "summary": [],
        "content": [{ "type": "reasoning_text", "text": state.text }],
    })
}

/// Encode one converted event in the inbound wire format. Claude and OpenAI
/// Responses streams carry `event:` names equal to the payload `type`; chat
/// completions and gemini (`alt=sse`) are data-only.
fn encode_frame(kind: ContentGenerationKind, v: &Value) -> String {
    use ContentGenerationKind as K;
    let data = v.to_string();
    match kind {
        K::ClaudeMessages | K::OpenAiResponses | K::OpenAiResponsesWebSocket => {
            let name = v.get("type").and_then(|t| t.as_str()).unwrap_or("message");
            SseFrame::event(name, data).encode()
        }
        K::OpenAiChatCompletions | K::GeminiGenerateContent => SseFrame::data(data).encode(),
    }
}

/// Turn one complete content-generation response into the smallest useful SSE
/// event sequence for the same wire format. This is the reverse of
/// [`aggregate_buffered`]: it is used when a streaming client is deliberately
/// routed to a non-streaming upstream operation.
pub fn synthesize_sse(
    kind: ContentGenerationKind,
    body: &[u8],
) -> Result<Vec<u8>, super::TransformError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| super::TransformError::InvalidInput {
            reason: format!("synthetic stream response is not JSON: {error}"),
        })?;
    let mut out = String::new();
    match kind {
        ContentGenerationKind::OpenAiChatCompletions => synthesize_chat(&value, &mut out),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => synthesize_responses(&value, &mut out),
        ContentGenerationKind::ClaudeMessages => synthesize_claude(&value, &mut out),
        ContentGenerationKind::GeminiGenerateContent => {
            out.push_str(&SseFrame::data(value.to_string()).encode());
        }
    }
    Ok(out.into_bytes())
}

fn synthesize_chat(response: &Value, out: &mut String) {
    let choices = response
        .get("choices")
        .and_then(Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(|choice| {
                    let mut delta = choice.get("message").cloned().unwrap_or_else(|| json!({}));
                    if let Some(object) = delta.as_object_mut() {
                        object.remove("annotations");
                    }
                    json!({
                        "index": choice.get("index").cloned().unwrap_or_else(|| json!(0)),
                        "delta": delta,
                        "finish_reason": choice.get("finish_reason").cloned().unwrap_or(Value::Null),
                        "logprobs": choice.get("logprobs").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut chunk = response.clone();
    if let Some(object) = chunk.as_object_mut() {
        object.insert("object".to_owned(), json!("chat.completion.chunk"));
        object.insert("choices".to_owned(), Value::Array(choices));
    }
    out.push_str(&SseFrame::data(chunk.to_string()).encode());
    out.push_str(&SseFrame::data("[DONE]").encode());
}

fn synthesize_responses(response: &Value, out: &mut String) {
    let mut started = response.clone();
    if let Some(object) = started.as_object_mut() {
        object.insert("status".to_owned(), json!("in_progress"));
        object.insert("output".to_owned(), json!([]));
    }
    push_named(out, json!({"type":"response.created","response":started}));

    for (output_index, item) in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut added_item = item.clone();
        if let Some(object) = added_item.as_object_mut() {
            object.insert("status".to_owned(), json!("in_progress"));
        }
        push_named(
            out,
            json!({"type":"response.output_item.added","output_index":output_index,"item":added_item}),
        );
        let item_id = item
            .get("id")
            .cloned()
            .unwrap_or_else(|| json!(format!("item_{output_index}")));
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for (content_index, part) in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    push_named(
                        out,
                        json!({
                            "type":"response.content_part.added",
                            "item_id":item_id,
                            "output_index":output_index,
                            "content_index":content_index,
                            "part":part,
                        }),
                    );
                    let (delta_type, done_type, field) =
                        match part.get("type").and_then(Value::as_str) {
                            Some("refusal") => {
                                ("response.refusal.delta", "response.refusal.done", "refusal")
                            }
                            _ => (
                                "response.output_text.delta",
                                "response.output_text.done",
                                "text",
                            ),
                        };
                    let text = part.get(field).cloned().unwrap_or_else(|| json!(""));
                    push_named(
                        out,
                        json!({
                            "type":delta_type,"item_id":item_id,"output_index":output_index,
                            "content_index":content_index,"delta":text,
                        }),
                    );
                    push_named(
                        out,
                        json!({
                            "type":done_type,"item_id":item_id,"output_index":output_index,
                            "content_index":content_index, (field):text,
                        }),
                    );
                    push_named(
                        out,
                        json!({
                            "type":"response.content_part.done","item_id":item_id,
                            "output_index":output_index,"content_index":content_index,"part":part,
                        }),
                    );
                }
            }
            Some("function_call") => {
                let arguments = item.get("arguments").cloned().unwrap_or_else(|| json!(""));
                push_named(
                    out,
                    json!({
                        "type":"response.function_call_arguments.delta","item_id":item_id,
                        "output_index":output_index,"delta":arguments,
                    }),
                );
                push_named(
                    out,
                    json!({
                        "type":"response.function_call_arguments.done","item_id":item_id,
                        "output_index":output_index,"name":item.get("name"),"arguments":arguments,
                    }),
                );
            }
            Some("custom_tool_call") => {
                let input = item.get("input").cloned().unwrap_or_else(|| json!(""));
                push_named(
                    out,
                    json!({
                        "type":"response.custom_tool_call_input.delta","item_id":item_id,
                        "output_index":output_index,"delta":input,
                    }),
                );
                push_named(
                    out,
                    json!({
                        "type":"response.custom_tool_call_input.done","item_id":item_id,
                        "output_index":output_index,"input":input,
                    }),
                );
            }
            Some("reasoning") => {
                for (content_index, part) in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if let Some(text) = part.get("text") {
                        push_named(
                            out,
                            json!({
                                "type":"response.reasoning_text.delta","item_id":item_id,
                                "output_index":output_index,"content_index":content_index,"delta":text,
                            }),
                        );
                        push_named(
                            out,
                            json!({
                                "type":"response.reasoning_text.done","item_id":item_id,
                                "output_index":output_index,"content_index":content_index,"text":text,
                            }),
                        );
                    }
                }
            }
            _ => {}
        }
        push_named(
            out,
            json!({"type":"response.output_item.done","output_index":output_index,"item":item}),
        );
    }
    push_named(
        out,
        json!({"type":"response.completed","response":response}),
    );
}

fn synthesize_claude(response: &Value, out: &mut String) {
    let mut message = response.clone();
    if let Some(object) = message.as_object_mut() {
        object.insert("content".to_owned(), json!([]));
        object.insert("stop_reason".to_owned(), Value::Null);
        object.insert("stop_sequence".to_owned(), Value::Null);
    }
    push_named(out, json!({"type":"message_start","message":message}));
    for (index, block) in response
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut start = block.clone();
        if let Some(object) = start.as_object_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    object.insert("text".to_owned(), json!(""));
                }
                Some("thinking") => {
                    object.insert("thinking".to_owned(), json!(""));
                }
                Some("tool_use") => {
                    object.insert("input".to_owned(), json!({}));
                }
                _ => {}
            }
        }
        push_named(
            out,
            json!({"type":"content_block_start","index":index,"content_block":start}),
        );
        match block.get("type").and_then(Value::as_str) {
            Some("text") => push_named(
                out,
                json!({
                    "type":"content_block_delta","index":index,
                    "delta":{"type":"text_delta","text":block.get("text").cloned().unwrap_or_else(|| json!(""))},
                }),
            ),
            Some("thinking") => {
                push_named(
                    out,
                    json!({
                        "type":"content_block_delta","index":index,
                        "delta":{"type":"thinking_delta","thinking":block.get("thinking").cloned().unwrap_or_else(|| json!(""))},
                    }),
                );
                if let Some(signature) = block.get("signature") {
                    push_named(
                        out,
                        json!({
                            "type":"content_block_delta","index":index,
                            "delta":{"type":"signature_delta","signature":signature},
                        }),
                    );
                }
            }
            Some("tool_use") => push_named(
                out,
                json!({
                    "type":"content_block_delta","index":index,
                    "delta":{"type":"input_json_delta","partial_json":block.get("input").cloned().unwrap_or_else(|| json!({})).to_string()},
                }),
            ),
            _ => {}
        }
        push_named(out, json!({"type":"content_block_stop","index":index}));
    }
    push_named(
        out,
        json!({
            "type":"message_delta",
            "delta":{
                "stop_reason":response.get("stop_reason").cloned().unwrap_or(Value::Null),
                "stop_sequence":response.get("stop_sequence").cloned().unwrap_or(Value::Null),
            },
            "usage":response.get("usage").cloned().unwrap_or_else(|| json!({})),
        }),
    );
    push_named(out, json!({"type":"message_stop"}));
}

fn push_named(out: &mut String, event: Value) {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    out.push_str(&SseFrame::event(name, event.to_string()).encode());
}

/// Convert a fully-buffered SSE body (wasm, or any buffered streaming attempt).
pub fn convert_buffered(mut t: SseTransformer, body: &[u8]) -> Vec<u8> {
    let mut out = t.push(body);
    out.extend(t.finish());
    out
}

/// Collapse a fully-buffered provider SSE stream into a single non-stream
/// response JSON of the **same** wire `kind`, reusing the per-format aggregators
/// in [`stream_to_response`](crate::transform::generate_content::stream_to_response).
///
/// Used when the routed target op is the streaming op but the client asked for a
/// non-stream response: the upstream still streamed, so its buffered body is SSE
/// that must be folded back into one object. Returns the original bytes on a
/// parse/serialize failure (best-effort — the caller already holds a body).
pub fn aggregate_buffered(kind: ContentGenerationKind, sse_body: &[u8]) -> Vec<u8> {
    use crate::transform::generate_content::stream_to_response as s2r;
    use ContentGenerationKind as K;

    // SSE bytes → frames; each frame's `data` is one event JSON. Skip the
    // openai-chat `[DONE]` terminator (not an event).
    let mut dec = SseDecoder::new();
    let mut frames = dec.push(sse_body);
    if let Some(tail) = dec.finish() {
        frames.push(tail);
    }
    let datas: Vec<String> = frames
        .into_iter()
        .map(|f| f.data)
        .filter(|d| d.trim() != "[DONE]")
        .collect();

    macro_rules! collapse {
        ($ty:ty, $agg:path) => {{
            let events = datas
                .iter()
                .filter_map(|d| serde_json::from_str::<$ty>(d.as_str()).ok());
            serde_json::to_vec(&$agg(events))
        }};
    }

    let out = match kind {
        K::OpenAiResponses | K::OpenAiResponsesWebSocket => collapse!(
            crate::protocol::openai::ResponseStreamEvent,
            s2r::openai_responses::response
        ),
        K::OpenAiChatCompletions => collapse!(
            crate::protocol::openai::ChatCompletionChunk,
            s2r::openai_chat::response
        ),
        K::ClaudeMessages => collapse!(
            crate::protocol::claude::StreamEvent,
            s2r::claude_messages::response
        ),
        K::GeminiGenerateContent => collapse!(
            crate::protocol::gemini::StreamGenerateContentChunk,
            s2r::gemini_generate_content::response
        ),
    };
    out.unwrap_or_else(|_| sse_body.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Operation, OperationKey};

    /// openai-chat upstream chunks → claude inbound events, across a chunk
    /// boundary, with [DONE] swallowed and claude event names emitted.
    #[test]
    fn chat_chunks_to_claude_events() {
        let upstream = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let inbound = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        let pair = crate::transform::resolve(upstream, inbound).unwrap();
        let mut t = SseTransformer::new(
            pair,
            TransformContext::new(upstream, inbound),
            ContentGenerationKind::ClaudeMessages,
        );
        let chunk = br#"data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"he"},"finish_reason":null}]}"#;
        let mut out = t.push(chunk);
        out.extend(t.push(b"\n\ndata: [DONE]\n\n"));
        out.extend(t.finish());
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("event: "),
            "claude frames carry event names: {text}"
        );
        assert!(
            !text.contains("[DONE]"),
            "claude streams have no DONE: {text}"
        );
        // every data line parses as JSON with a type field
        for line in text.lines().filter(|l| l.starts_with("data: ")) {
            let v: Value = serde_json::from_str(&line[6..]).unwrap();
            assert!(v.get("type").is_some());
        }
    }

    /// Buffered chat SSE (two delta chunks + [DONE]) collapses to a single
    /// `chat.completion` object with the concatenated content.
    #[test]
    fn aggregate_buffered_collapses_chat() {
        let sse = concat!(
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"he\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"llo\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let out = aggregate_buffered(ContentGenerationKind::OpenAiChatCompletions, sse.as_bytes());
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["object"], "chat.completion",
            "collapsed to a response: {v}"
        );
        assert_eq!(v["choices"][0]["message"]["content"], "hello");
    }

    #[test]
    fn complete_chat_response_becomes_one_chunk_and_done() {
        let response = json!({
            "id":"chat_1","object":"chat.completion","created":1,"model":"m",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
        });
        let out = synthesize_sse(
            ContentGenerationKind::OpenAiChatCompletions,
            response.to_string().as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("chat.completion.chunk"));
        assert!(text.contains(r#""content":"hello""#));
        assert!(text.ends_with("data: [DONE]\n\n"));
    }

    #[test]
    fn complete_claude_response_preserves_text_and_tool_input() {
        let response = json!({
            "id":"msg_1","type":"message","role":"assistant","model":"m",
            "content":[
                {"type":"text","text":"hello"},
                {"type":"tool_use","id":"tool_1","name":"echo","input":{"text":"hi"}}
            ],
            "stop_reason":"tool_use","stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":2}
        });
        let out = synthesize_sse(
            ContentGenerationKind::ClaudeMessages,
            response.to_string().as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("event: message_start"));
        assert!(text.contains(r#""text":"hello""#));
        assert!(text.contains(r#""type":"text_delta""#));
        assert!(text.contains(r#""partial_json":"{\"text\":\"hi\"}""#));
        assert!(text.ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
    }

    #[test]
    fn complete_responses_object_emits_deltas_tools_and_completed() {
        let response = json!({
            "id":"resp_1","object":"response","status":"completed","model":"m",
            "output":[
                {"id":"msg_1","type":"message","status":"completed","role":"assistant",
                 "content":[{"type":"output_text","text":"hello","annotations":[]}]},
                {"id":"fc_1","type":"function_call","status":"completed","call_id":"call_1",
                 "name":"echo","arguments":"{\"text\":\"hi\"}"}
            ]
        });
        let out = synthesize_sse(
            ContentGenerationKind::OpenAiResponses,
            response.to_string().as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("event: response.output_text.delta"));
        assert!(text.contains(r#""text":"hello""#));
        assert!(text.contains("event: response.function_call_arguments.done"));
        assert!(text.contains("event: response.completed"));
    }

    #[test]
    fn chat_tool_call_stream_finishes_responses_item() {
        let upstream = OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let inbound = OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        );
        let pair = crate::transform::resolve(upstream, inbound).unwrap();
        let mut t = SseTransformer::new(
            pair,
            TransformContext::new(upstream, inbound),
            ContentGenerationKind::OpenAiResponses,
        );

        let mut out = t.push(br#"data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"echo_text","arguments":""}}]},"finish_reason":null}]}"#);
        out.extend(t.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":\"hello\"}"}}]},"finish_reason":null}]}"#));
        out.extend(t.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#));
        out.extend(t.push(b"\n\ndata: [DONE]\n\n"));
        out.extend(t.finish());

        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("event: response.function_call_arguments.done"),
            "function arguments are completed: {text}"
        );
        assert!(
            text.contains("event: response.output_item.done"),
            "function item is completed: {text}"
        );
        assert!(
            text.contains(r#""arguments":"{\"text\":\"hello\"}""#),
            "full arguments are preserved: {text}"
        );
        assert!(
            !text.contains(r#""item_id":"fc_0""#),
            "argument deltas use the announced function item id: {text}"
        );
        assert!(
            text.contains(r#""output":[{"arguments":"{\"text\":\"hello\"}""#),
            "response.completed carries the function output item: {text}"
        );
    }
}
