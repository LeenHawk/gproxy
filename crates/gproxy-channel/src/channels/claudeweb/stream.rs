//! claude.ai completion SSE -> Claude Messages SSE (v3 `claudeweb/stream/`
//! `codec.rs`, `modern.rs`, `sse.rs`).
//!
//! The modern web stream already speaks Messages event types; the codec
//! fills in what the web front end omits (a canonical `message_start`, the
//! character-estimated `usage`), hides the echoed `tool_result` blocks and
//! closes the message with `stop_reason: tool_use` at a tool boundary so the
//! client can run the tool. Legacy `{"completion": ...}` frames without a
//! `type` become one text block. Frames arrive as JSON values so the same
//! codec feeds the SSE encoder and the non-streaming collector.

use gproxy_protocol::{
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::Bytes,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::request::estimate_tokens;
use crate::channel::ChannelError;

/// Bounds for the upstream SSE decoder; the host enforces transfer limits.
pub(super) const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 16 * 1024 * 1024,
    max_value_bytes: 16 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 16 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

/// What a continuation must remember between the tool boundary and the
/// tool result; this is the JSON stored under `tool:{tool_use_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub conversation: String,
    pub model: String,
    pub message_id: String,
    pub input_tokens: u64,
}

pub(super) struct Output {
    pub events: Vec<Value>,
    /// The `tool_use` id the message stopped at, when it did.
    pub tool_use: Option<String>,
}

pub(super) struct Codec {
    decoder: SseDecoder,
    /// Events decoded after a tool boundary in the same chunk; replayed on resume.
    carry: Vec<Value>,
    state: SessionState,
    output: String,
    tool_id: Option<String>,
    tool_index: Option<u64>,
    skipped_result: Option<u64>,
    legacy: bool,
    started: bool,
    stopped: bool,
    resume_start: bool,
}

impl Codec {
    pub(super) fn new(state: SessionState) -> Self {
        Self {
            decoder: SseDecoder::new(SSE_LIMITS),
            carry: Vec::new(),
            state,
            output: String::new(),
            tool_id: None,
            tool_index: None,
            skipped_result: None,
            legacy: false,
            started: false,
            stopped: false,
            resume_start: false,
        }
    }

    /// Continue the same upstream stream as a new Messages message: the
    /// resumed message opens with its own `message_start`.
    pub(super) fn resume(&mut self, state: SessionState) {
        self.state = state;
        self.output.clear();
        self.tool_id = None;
        self.tool_index = None;
        self.skipped_result = None;
        self.started = false;
        self.stopped = false;
        self.resume_start = true;
    }

    pub(super) fn state(&self) -> &SessionState {
        &self.state
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Output, ChannelError> {
        let frames = self
            .decoder
            .push(chunk)
            .map_err(|error| ChannelError::InvalidResponse(format!("claude.ai SSE: {error}")))?;
        let mut values = std::mem::take(&mut self.carry);
        for frame in frames {
            if let SseFrame::Event(event) = frame
                && !event.data.trim().is_empty()
            {
                values.push(serde_json::from_str(&event.data).map_err(|error| {
                    ChannelError::InvalidResponse(format!("claude.ai SSE JSON: {error}"))
                })?);
            }
        }
        self.events(values)
    }

    /// Upstream EOF: close the message if the web stream never did.
    pub(super) fn finish(&mut self) -> Result<Vec<Value>, ChannelError> {
        let mut values = std::mem::take(&mut self.carry);
        if let Ok(frames) = self.decoder.finish() {
            for frame in frames {
                if let SseFrame::Event(event) = frame
                    && let Ok(value) = serde_json::from_str::<Value>(&event.data)
                {
                    values.push(value);
                }
            }
        }
        let mut output = self.events(values)?.events;
        if self.legacy && !self.stopped {
            output.push(json!({"type": "content_block_stop", "index": 0}));
        }
        if !self.stopped {
            output.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                "usage": {"output_tokens": self.output_tokens()}
            }));
            output.push(json!({"type": "message_stop"}));
            self.stopped = true;
        }
        Ok(output)
    }

    fn events(&mut self, values: Vec<Value>) -> Result<Output, ChannelError> {
        let mut events = Vec::new();
        if self.resume_start {
            self.resume_start = false;
            self.started = true;
            events.push(self.message_start());
        }
        let mut values = values.into_iter();
        while let Some(mut value) = values.next() {
            if value.get("type").is_none() {
                self.legacy = true;
                self.legacy_event(&value, &mut events);
                continue;
            }
            if let Some(tool_use) = self.modern(&mut value, &mut events)? {
                self.carry = values.collect();
                return Ok(Output {
                    events,
                    tool_use: Some(tool_use),
                });
            }
        }
        Ok(Output {
            events,
            tool_use: None,
        })
    }

    fn legacy_event(&mut self, value: &Value, events: &mut Vec<Value>) {
        if !self.started {
            self.started = true;
            events.push(self.message_start());
            events.push(json!({
                "type": "content_block_start", "index": 0,
                "content_block": {"type": "text", "text": ""}
            }));
        }
        if let Some(delta) = value.get("completion").and_then(Value::as_str) {
            self.output.push_str(delta);
            events.push(json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": delta}
            }));
        }
    }

    /// Port of v3 `stream/modern.rs`: returns the tool_use id when the
    /// message must stop for a tool call.
    fn modern(
        &mut self,
        value: &mut Value,
        events: &mut Vec<Value>,
    ) -> Result<Option<String>, ChannelError> {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let index = value.get("index").and_then(Value::as_u64);
        if kind == "content_block_start" {
            match value.pointer("/content_block/type").and_then(Value::as_str) {
                Some("tool_result") => {
                    self.skipped_result = index;
                    return Ok(None);
                }
                Some("tool_use") => {
                    self.tool_index = index;
                    self.tool_id = value
                        .pointer("/content_block/id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                _ => {}
            }
        }
        if self.skipped_result.is_some() && self.skipped_result == index {
            if kind == "content_block_stop" {
                self.skipped_result = None;
            }
            return Ok(None);
        }
        if !self.started
            && matches!(
                kind.as_str(),
                "content_block_start"
                    | "content_block_delta"
                    | "content_block_stop"
                    | "message_delta"
                    | "message_stop"
            )
        {
            self.started = true;
            events.push(self.message_start());
        }
        match kind.as_str() {
            "message_start" => {
                self.started = true;
                if let Some(message) = value.get_mut("message").and_then(Value::as_object_mut) {
                    message.insert(
                        "usage".into(),
                        json!({"input_tokens": self.state.input_tokens, "output_tokens": 0}),
                    );
                    for (key, value) in [
                        ("type", Value::String("message".into())),
                        ("role", Value::String("assistant".into())),
                        ("model", Value::String(self.state.model.clone())),
                        ("stop_reason", Value::Null),
                        ("stop_sequence", Value::Null),
                    ] {
                        message.entry(key).or_insert(value);
                    }
                    if let Some(id) = message.get("id").and_then(Value::as_str) {
                        self.state.message_id = id.into();
                    }
                }
            }
            "content_block_delta" => self.count_delta(value),
            "message_delta" => {
                value["usage"] = json!({"output_tokens": self.output_tokens()});
            }
            "message_stop" => self.stopped = true,
            _ => {}
        }
        events.push(value.clone());
        if kind == "content_block_stop" && self.tool_index.is_some() && self.tool_index == index {
            self.tool_index = None;
            self.state.input_tokens = self.state.input_tokens.saturating_add(self.output_tokens());
            events.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": "tool_use", "stop_sequence": null},
                "usage": {"output_tokens": self.output_tokens()}
            }));
            events.push(json!({"type": "message_stop"}));
            self.stopped = true;
            return self
                .tool_id
                .take()
                .map(Some)
                .ok_or_else(|| ChannelError::InvalidResponse("tool_use id missing".into()));
        }
        Ok(None)
    }

    fn count_delta(&mut self, value: &Value) {
        if let Some(text) = value
            .get("delta")
            .and_then(|delta| {
                delta
                    .get("text")
                    .or_else(|| delta.get("thinking"))
                    .or_else(|| delta.get("partial_json"))
            })
            .and_then(Value::as_str)
        {
            self.output.push_str(text);
        }
    }

    fn message_start(&self) -> Value {
        json!({
            "type": "message_start",
            "message": {
                "id": self.state.message_id, "type": "message", "role": "assistant",
                "content": [], "model": self.state.model,
                "stop_reason": null, "stop_sequence": null,
                "usage": {"input_tokens": self.state.input_tokens, "output_tokens": 0}
            }
        })
    }

    fn output_tokens(&self) -> u64 {
        estimate_tokens(&self.output)
    }
}

/// `event: {type}\ndata: {json}\n\n`, the Claude Messages framing.
pub(super) fn encode(value: &Value) -> Bytes {
    let event = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    Bytes::from(format!("event: {event}\ndata: {value}\n\n"))
}

pub(super) fn encode_all(values: &[Value]) -> Bytes {
    let mut out = Vec::new();
    for value in values {
        out.extend_from_slice(&encode(value));
    }
    Bytes::from(out)
}

/// Folds translated Messages events into one Messages response body.
#[derive(Default)]
pub(super) struct Collector {
    message: Option<Value>,
    blocks: Vec<Value>,
    partial_json: Vec<String>,
}

impl Collector {
    pub(super) fn push(&mut self, event: &Value) -> Result<(), ChannelError> {
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                let mut message = event.get("message").cloned().unwrap_or_else(|| json!({}));
                if let Some(object) = message.as_object_mut() {
                    object.remove("content");
                }
                self.message = Some(message);
            }
            Some("content_block_start") => {
                let mut block = event
                    .get("content_block")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "text", "text": ""}));
                if block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && block.get("input").is_none()
                {
                    block["input"] = json!({});
                }
                self.blocks.push(block);
                self.partial_json.push(String::new());
            }
            Some("content_block_delta") => {
                let Some(delta) = event.get("delta") else {
                    return Ok(());
                };
                let index = self.index(event)?;
                let block = &mut self.blocks[index];
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => append(block, "text", delta.get("text")),
                    Some("thinking_delta") => append(block, "thinking", delta.get("thinking")),
                    Some("signature_delta") => {
                        if let Some(signature) = delta.get("signature") {
                            block["signature"] = signature.clone();
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(partial) = delta.get("partial_json").and_then(Value::as_str) {
                            self.partial_json[index].push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                let index = self.index(event)?;
                let json = std::mem::take(&mut self.partial_json[index]);
                if !json.trim().is_empty() {
                    let input: Value = serde_json::from_str(&json).map_err(|error| {
                        ChannelError::InvalidResponse(format!("tool_use input: {error}"))
                    })?;
                    self.blocks[index]["input"] = input;
                }
            }
            Some("message_delta") => {
                let message = self.message.get_or_insert_with(|| json!({}));
                if let Some(delta) = event.get("delta").and_then(Value::as_object) {
                    for (key, value) in delta {
                        message[key] = value.clone();
                    }
                }
                if let Some(output) = event.pointer("/usage/output_tokens") {
                    message["usage"]["output_tokens"] = output.clone();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Value {
        let mut message = self.message.unwrap_or_else(|| json!({}));
        message["content"] = Value::Array(self.blocks);
        message
    }

    fn index(&self, event: &Value) -> Result<usize, ChannelError> {
        let index = event
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(self.blocks.len().saturating_sub(1));
        if index < self.blocks.len() {
            Ok(index)
        } else {
            Err(ChannelError::InvalidResponse(
                "content block delta before its start".into(),
            ))
        }
    }
}

fn append(block: &mut Value, field: &str, delta: Option<&Value>) {
    if let Some(text) = delta.and_then(Value::as_str) {
        let current = block.get(field).and_then(Value::as_str).unwrap_or_default();
        block[field] = Value::String(format!("{current}{text}"));
    }
}
