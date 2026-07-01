//! Runtime SSE adapter for cross-protocol streaming: decode upstream frames,
//! convert each event through the resolved (reverse) pair, re-encode in the
//! inbound wire format. Sync core — shared by the native stream wrapper
//! (`pipeline/stream.rs`) and the buffered path (wasm / buffered attempts).
//!
//! Pair `stream_event` fns are 1:1 and stateless; any future cross-event
//! aggregation (block indexes, tool-call identity, final usage) lives HERE
//! (see transform/README.md).

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

impl SseTransformer {
    pub fn new(pair: TransformPair, ctx: TransformContext, inbound: ContentGenerationKind) -> Self {
        Self {
            decoder: SseDecoder::new(),
            pair,
            ctx,
            inbound,
            responses: (inbound == ContentGenerationKind::OpenAiResponses)
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

impl ResponsesStreamState {
    fn push(&mut self, event: Value) -> Vec<Value> {
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
            Some("response.completed") => {
                let mut out = self.finish_reasoning();
                out.extend(self.finish_message());
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
            _ => {}
        }
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
        K::ClaudeMessages | K::OpenAiResponses => {
            let name = v.get("type").and_then(|t| t.as_str()).unwrap_or("message");
            SseFrame::event(name, data).encode()
        }
        K::OpenAiChatCompletions | K::GeminiGenerateContent => SseFrame::data(data).encode(),
    }
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
        K::OpenAiResponses => collapse!(
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
}
