//! CodeWhisperer event-stream frames translated into OpenAI Responses SSE.
//!
//! `GenerateAssistantResponse` answers in AWS `vnd.amazon.eventstream` framing
//! whose payloads are the CodeWhisperer event objects, not the Responses
//! events a client asked for. The translator turns one into the other as the
//! frames arrive (v3 `kiro/decoder/`): text and reasoning deltas are deduped
//! and percent-decoded, tool calls become Responses function-call items, and
//! the terminal `response.completed` carries the whole answer plus the usage
//! the `metadataEvent` reported.
//!
//! A buffered caller gets the same work: `completed()` is the response object
//! the terminal event carries, so `generate_content` needs no second parser.

mod sse;
mod tools;

use super::usage;
use crate::channel::ChannelError;
use crate::channels::shared::aws_eventstream::FrameParser;
use gproxy_protocol::connection::Bytes;
use serde_json::{Value, json};

/// Event-stream frames in, Responses SSE bytes out.
pub(super) struct Translator {
    parser: FrameParser,
    response_id: String,
    message_id: String,
    reasoning_id: String,
    model: String,
    content: String,
    reasoning: String,
    last_content: String,
    last_reasoning: String,
    usage: Option<Value>,
    started: bool,
    content_started: bool,
    reasoning_started: bool,
    failed: bool,
    tools: tools::Tracker,
    sequence: u64,
    completed: Option<Value>,
}

impl Translator {
    pub(super) fn new(response_id: &str, model: &str) -> Self {
        Self {
            parser: FrameParser::new(),
            message_id: sse::id("msg", response_id),
            reasoning_id: sse::id("rs", response_id),
            response_id: response_id.to_owned(),
            model: model.to_owned(),
            content: String::new(),
            reasoning: String::new(),
            last_content: String::new(),
            last_reasoning: String::new(),
            usage: None,
            started: false,
            content_started: false,
            reasoning_started: false,
            failed: false,
            tools: tools::Tracker::default(),
            sequence: 0,
            completed: None,
        }
    }

    /// True once an exception event ended the answer; the caller stops reading
    /// the upstream body.
    pub(super) fn failed(&self) -> bool {
        self.failed
    }

    /// The `response` object of the terminal event, available after `finish`.
    pub(super) fn completed(&self) -> Option<&Value> {
        self.completed.as_ref()
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Bytes, ChannelError> {
        let mut out = Vec::new();
        for frame in self.parser.push(chunk)? {
            let name = frame.name().to_owned();
            let value: Value = serde_json::from_slice(&frame.payload)
                .map_err(|error| decode(format!("event payload JSON: {error}")))?;
            // The payload is either the event object itself or a single-key
            // envelope naming the event (v3 `decoder/events.rs`).
            let payload = value.get(&name).unwrap_or(&value).clone();
            self.event(&name, &payload, &mut out)?;
            if self.failed {
                break;
            }
        }
        Ok(Bytes::from(out))
    }

    /// Upstream EOF. Emits the terminal events and records the completed
    /// response; an answer that never started, ended mid-tool-call or ended in
    /// an exception is refused.
    pub(super) fn finish(&mut self) -> Result<Bytes, ChannelError> {
        if self.failed {
            return Err(decode("the upstream ended with an exception event"));
        }
        self.parser.finish()?;
        if !self.tools.is_complete() {
            return Err(decode("the stream ended before a tool call stopped"));
        }
        if !self.started {
            return Err(decode("the stream produced no events"));
        }
        Ok(Bytes::from(self.terminal()))
    }

    fn event(
        &mut self,
        name: &str,
        payload: &Value,
        out: &mut Vec<u8>,
    ) -> Result<(), ChannelError> {
        match name {
            "assistantResponseEvent" => {
                let Some(text) = payload.get("content").and_then(Value::as_str) else {
                    return Ok(());
                };
                self.text(text, out);
            }
            "reasoningContentEvent" => {
                let Some(text) = payload
                    .get("text")
                    .or_else(|| payload.get("content"))
                    .and_then(Value::as_str)
                else {
                    return Ok(());
                };
                self.thinking(text, out);
            }
            "metadataEvent" => {
                if let Some(reported) = payload.get("tokenUsage").and_then(usage::response_usage) {
                    self.usage = Some(reported);
                }
            }
            // Only before the first delta: the ids are already on the wire
            // after that and renaming them would break the client's items.
            "messageMetadataEvent" if !self.started => {
                if let Some(id) = payload
                    .get("conversationId")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                {
                    self.response_id = id.to_owned();
                    self.message_id = sse::id("msg", id);
                    self.reasoning_id = sse::id("rs", id);
                }
            }
            "toolUseEvent" => {
                self.ensure_started(out);
                for frame in self.tools.handle(payload, &mut self.sequence)? {
                    out.extend_from_slice(&frame);
                }
            }
            "invalidStateEvent" | "InternalServerException" | "internalServerException" => {
                self.ensure_started(out);
                self.failed = true;
                let sequence = self.take();
                let message = payload
                    .get("message")
                    .or_else(|| payload.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("Kiro stream failed");
                self.emit(
                    out,
                    &json!({
                        "type": "error", "sequence_number": sequence,
                        "code": "kiro_eventstream_error", "param": null, "message": message,
                    }),
                );
            }
            _ => {}
        }
        Ok(())
    }

    fn text(&mut self, value: &str, out: &mut Vec<u8>) {
        let delta = sse::percent_decode(&sse::dedup(value, &mut self.last_content));
        if delta.is_empty() {
            return;
        }
        self.ensure_started(out);
        if !self.content_started {
            self.content_started = true;
            let added = self.take();
            let part = self.take();
            let item = sse::message(&self.message_id, "", "in_progress");
            self.emit(
                out,
                &json!({"type": "response.output_item.added", "sequence_number": added,
                        "output_index": 0, "item": item}),
            );
            let item_id = self.message_id.clone();
            self.emit(
                out,
                &json!({"type": "response.content_part.added", "sequence_number": part,
                        "output_index": 0, "item_id": item_id, "content_index": 0,
                        "part": {"type": "output_text", "text": "", "annotations": []}}),
            );
        }
        self.content.push_str(&delta);
        let sequence = self.take();
        let item_id = self.message_id.clone();
        self.emit(
            out,
            &json!({"type": "response.output_text.delta", "sequence_number": sequence,
                    "output_index": 0, "item_id": item_id, "content_index": 0, "delta": delta}),
        );
    }

    fn thinking(&mut self, value: &str, out: &mut Vec<u8>) {
        let delta = sse::percent_decode(&sse::dedup(value, &mut self.last_reasoning));
        if delta.is_empty() {
            return;
        }
        self.ensure_started(out);
        if !self.reasoning_started {
            self.reasoning_started = true;
            let sequence = self.take();
            let item = sse::reasoning(&self.reasoning_id, "", "in_progress");
            self.emit(
                out,
                &json!({"type": "response.output_item.added", "sequence_number": sequence,
                        "output_index": 1, "item": item}),
            );
        }
        self.reasoning.push_str(&delta);
        let sequence = self.take();
        let item_id = self.reasoning_id.clone();
        self.emit(
            out,
            &json!({"type": "response.reasoning_text.delta", "sequence_number": sequence,
                    "output_index": 1, "item_id": item_id, "content_index": 0, "delta": delta}),
        );
    }

    fn ensure_started(&mut self, out: &mut Vec<u8>) {
        if self.started {
            return;
        }
        self.started = true;
        let sequence = self.take();
        let response = sse::response(&self.response_id, &self.model, "in_progress", Vec::new());
        self.emit(
            out,
            &json!({"type": "response.created", "sequence_number": sequence,
                    "response": response}),
        );
    }

    /// The `done` events for whatever opened, then `response.completed`.
    fn terminal(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let message_id = self.message_id.clone();
        let reasoning_id = self.reasoning_id.clone();
        let content = self.content.clone();
        let reasoning = self.reasoning.clone();
        if self.content_started {
            let text_done = self.take();
            let part_done = self.take();
            let item_done = self.take();
            self.emit(
                &mut out,
                &json!({"type": "response.output_text.done", "sequence_number": text_done,
                        "output_index": 0, "item_id": message_id, "content_index": 0,
                        "text": content}),
            );
            self.emit(
                &mut out,
                &json!({"type": "response.content_part.done", "sequence_number": part_done,
                        "output_index": 0, "item_id": message_id, "content_index": 0,
                        "part": {"type": "output_text", "text": content, "annotations": []}}),
            );
            let item = sse::message(&message_id, &content, "completed");
            self.emit(
                &mut out,
                &json!({"type": "response.output_item.done", "sequence_number": item_done,
                        "output_index": 0, "item": item}),
            );
        }
        if self.reasoning_started {
            let text_done = self.take();
            let item_done = self.take();
            self.emit(
                &mut out,
                &json!({"type": "response.reasoning_text.done", "sequence_number": text_done,
                        "output_index": 1, "item_id": reasoning_id, "content_index": 0,
                        "text": reasoning}),
            );
            let item = sse::reasoning(&reasoning_id, &reasoning, "completed");
            self.emit(
                &mut out,
                &json!({"type": "response.output_item.done", "sequence_number": item_done,
                        "output_index": 1, "item": item}),
            );
        }
        let mut output = Vec::new();
        if self.content_started {
            output.push(sse::message(&message_id, &content, "completed"));
        }
        if self.reasoning_started {
            output.push(sse::reasoning(&reasoning_id, &reasoning, "completed"));
        }
        output.extend(self.tools.items());
        let mut response = sse::response(&self.response_id, &self.model, "completed", output);
        if self.content_started {
            response["output_text"] = Value::String(content);
        }
        if let Some(usage) = self.usage.clone() {
            response["usage"] = usage;
        }
        let sequence = self.take();
        self.emit(
            &mut out,
            &json!({"type": "response.completed", "sequence_number": sequence,
                    "response": response}),
        );
        self.completed = Some(response);
        out
    }

    fn emit(&self, out: &mut Vec<u8>, value: &Value) {
        out.extend_from_slice(&sse::frame(value));
    }

    fn take(&mut self) -> u64 {
        let sequence = self.sequence;
        self.sequence += 1;
        sequence
    }
}

fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("Kiro stream: {}", message.into()))
}
