//! SSE v1 delta decoder for `/backend-api/f/conversation`.
//!
//! The server streams events of several shapes under the `v1` encoding:
//!
//! * `event: delta_encoding / data: "v1"` — version banner, consumed once.
//! * `data: {type: "...", ...}` — typed side events (resume token, message
//!   marker, async status). Informational only, we pass through.
//! * `data: {p, o, v, c}` — initial "add" of a new state slot (channel `c`).
//!   `v` is a full message wrapper `{message, conversation_id, error, ...}`.
//! * `data: {p, o, v}` — single patch on the most-recently-added channel.
//! * `data: {o: "patch", v: [{p,o,v}, ...]}` — batch of patches on the
//!   most-recently-added channel.
//! * `data: {v: [{p,o,v}, ...]}` — batch without explicit `o: "patch"`
//!   (same semantics).
//!
//! This module parses raw SSE bytes into strongly-typed [`Event`] values.
//! Higher layers decide what to do with them (assembling OpenAI chunks,
//! collecting image file pointers, etc.).

use std::collections::VecDeque;

use serde::Deserialize;
use serde_json::Value;

/// One decoded SSE event.
#[derive(Debug, Clone)]
pub enum Event {
    /// `event: delta_encoding` banner. Carries the encoding name ("v1").
    Encoding(String),
    /// Typed side event (`data: {type, ...}`).
    Typed { kind: String, raw: Value },
    /// A delta patch. `channel` is `Some(c)` when the event was an "add"
    /// that declared a new channel; `None` for follow-up patches on the
    /// current channel.
    Delta(Delta),
    /// Stream finished cleanly (`data: [DONE]`).
    Done,
}

/// One or more JSON-Patch-like operations against channel state.
#[derive(Debug, Clone)]
pub struct Delta {
    /// Channel index declared by an "add" event, if any.
    pub channel: Option<u64>,
    /// Patch operations in application order.
    pub patches: Vec<PatchOp>,
}

#[derive(Debug, Clone)]
pub struct PatchOp {
    pub path: String,
    pub op: PatchKind,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchKind {
    Add,
    Replace,
    Append,
    Patch,
    Unknown,
}

impl PatchKind {
    fn parse(s: &str) -> Self {
        match s {
            "add" => Self::Add,
            "replace" => Self::Replace,
            "append" => Self::Append,
            "patch" => Self::Patch,
            _ => Self::Unknown,
        }
    }
}

/// Streaming byte-level decoder. Feed `feed()` with chunks from the HTTP
/// response body; pop fully-parsed events with `next_event()`.
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// Accumulated bytes for the current event block.
    current_event: Option<String>,
    data_buf: String,
    line_buf: String,
    events: VecDeque<Event>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of response bytes. Invalid UTF-8 is replaced.
    pub fn feed(&mut self, bytes: &[u8]) {
        let s = std::str::from_utf8(bytes)
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or_else(|_| String::from_utf8_lossy(bytes));
        self.line_buf.push_str(&s);
        while let Some(idx) = self.line_buf.find('\n') {
            let line = self.line_buf[..idx].to_string();
            self.line_buf.drain(..=idx);
            self.handle_line(line.trim_end_matches('\r'));
        }
    }

    fn handle_line(&mut self, line: &str) {
        if line.is_empty() {
            self.dispatch_block();
            return;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            self.current_event = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !self.data_buf.is_empty() {
                self.data_buf.push('\n');
            }
            self.data_buf.push_str(rest.trim_start());
        }
        // Comments (`:foo`) and unknown fields are skipped.
    }

    fn dispatch_block(&mut self) {
        let event_name = self.current_event.take();
        let data = std::mem::take(&mut self.data_buf);
        if data.is_empty() {
            return;
        }
        if let Some(parsed) = parse_block(event_name.as_deref(), &data) {
            self.events.push_back(parsed);
        }
    }

    /// Pop the next fully-parsed event, if any.
    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Drain all currently-buffered events.
    pub fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.events.drain(..)
    }
}

fn parse_block(event: Option<&str>, data: &str) -> Option<Event> {
    if data.trim() == "[DONE]" {
        return Some(Event::Done);
    }

    // `event: delta_encoding` banner. Body is a JSON string "v1".
    if matches!(event, Some("delta_encoding")) {
        let name = serde_json::from_str::<String>(data.trim())
            .unwrap_or_else(|_| data.trim().to_string());
        return Some(Event::Encoding(name));
    }

    let parsed: Value = serde_json::from_str(data).ok()?;

    // Typed event: `data: {"type": "...", ...}` (no `v` / `p` / `o` fields).
    if let Some(kind) = parsed.get("type").and_then(|v| v.as_str()) {
        // `type` fields also appear on some deltas; those have a `v`.
        if parsed.get("v").is_none() && parsed.get("p").is_none() {
            return Some(Event::Typed {
                kind: kind.to_string(),
                raw: parsed,
            });
        }
    }

    Some(Event::Delta(parse_delta(&parsed)))
}

fn parse_delta(v: &Value) -> Delta {
    let channel = v.get("c").and_then(|c| c.as_u64());

    // Case A: batch patch operation. `o` is "patch" and `v` is an array.
    let op = v.get("o").and_then(|x| x.as_str()).unwrap_or("");
    if op == "patch" {
        if let Some(arr) = v.get("v").and_then(|x| x.as_array()) {
            return Delta {
                channel,
                patches: arr.iter().filter_map(parse_one_patch).collect(),
            };
        }
    }

    // Case B: shorthand batch — no `o`/`p`, `v` is an array of patches.
    if v.get("o").is_none() && v.get("p").is_none() {
        if let Some(arr) = v.get("v").and_then(|x| x.as_array()) {
            return Delta {
                channel,
                patches: arr.iter().filter_map(parse_one_patch).collect(),
            };
        }

        // Case B': implicit "add" — no `o`/`p`, `v` is an object wrapper,
        // typically with a `c` channel marker. This is how the server
        // sends follow-up channel "adds" (channels 1..N) after the first.
        if let Some(obj_value) = v.get("v") {
            if obj_value.is_object() {
                return Delta {
                    channel,
                    patches: vec![PatchOp {
                        path: String::new(),
                        op: PatchKind::Add,
                        value: obj_value.clone(),
                    }],
                };
            }
        }
    }

    // Case C: single patch.
    let patch = PatchOp {
        path: v.get("p").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        op: PatchKind::parse(op),
        value: v.get("v").cloned().unwrap_or(Value::Null),
    };
    Delta {
        channel,
        patches: vec![patch],
    }
}

fn parse_one_patch(v: &Value) -> Option<PatchOp> {
    let obj = v.as_object()?;
    Some(PatchOp {
        path: obj
            .get("p")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        op: PatchKind::parse(obj.get("o").and_then(|x| x.as_str()).unwrap_or("")),
        value: obj.get("v").cloned().unwrap_or(Value::Null),
    })
}

/// Deserializable shape for the "initial add" event body. The `v` object
/// wraps a message + conversation metadata. Higher layers use it to
/// decide which channel is the assistant's final answer.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InitialAddValue {
    #[serde(default)]
    pub message: Option<Value>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub error: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_version_banner() {
        let mut d = SseDecoder::new();
        d.feed(b"event: delta_encoding\ndata: \"v1\"\n\n");
        match d.next_event().unwrap() {
            Event::Encoding(v) => assert_eq!(v, "v1"),
            other => panic!("expected encoding, got {other:?}"),
        }
    }

    #[test]
    fn decodes_initial_add() {
        let mut d = SseDecoder::new();
        d.feed(
            b"event: delta\ndata: {\"p\":\"\",\"o\":\"add\",\"v\":{\"message\":{\"id\":\"m1\"},\"conversation_id\":\"c1\"},\"c\":0}\n\n",
        );
        let ev = d.next_event().unwrap();
        if let Event::Delta(delta) = ev {
            assert_eq!(delta.channel, Some(0));
            assert_eq!(delta.patches.len(), 1);
            assert_eq!(delta.patches[0].op, PatchKind::Add);
        } else {
            panic!("expected delta");
        }
    }

    #[test]
    fn decodes_shorthand_batch_append() {
        let mut d = SseDecoder::new();
        d.feed(
            b"event: delta\ndata: {\"v\":[{\"p\":\"/message/content/parts/0\",\"o\":\"append\",\"v\":\"hello\"},{\"p\":\"/message/metadata/token_count\",\"o\":\"replace\",\"v\":7}]}\n\n",
        );
        let ev = d.next_event().unwrap();
        if let Event::Delta(delta) = ev {
            assert_eq!(delta.patches.len(), 2);
            assert_eq!(delta.patches[0].op, PatchKind::Append);
            assert_eq!(delta.patches[0].path, "/message/content/parts/0");
            assert_eq!(delta.patches[0].value, serde_json::json!("hello"));
        } else {
            panic!("expected delta");
        }
    }

    #[test]
    fn decodes_typed_event() {
        let mut d = SseDecoder::new();
        d.feed(b"data: {\"type\":\"message_marker\",\"marker\":\"first\"}\n\n");
        let ev = d.next_event().unwrap();
        if let Event::Typed { kind, .. } = ev {
            assert_eq!(kind, "message_marker");
        } else {
            panic!("expected typed");
        }
    }

    #[test]
    fn decodes_done() {
        let mut d = SseDecoder::new();
        d.feed(b"data: [DONE]\n\n");
        assert!(matches!(d.next_event(), Some(Event::Done)));
    }

    #[test]
    fn handles_multiple_chunks_split_mid_event() {
        let mut d = SseDecoder::new();
        d.feed(b"data: {\"v\":[{\"p\":\"/x\",\"o\":\"append\",\"");
        d.feed(b"v\":\"hi\"}]}\n\n");
        let ev = d.next_event().unwrap();
        if let Event::Delta(delta) = ev {
            assert_eq!(delta.patches[0].value, serde_json::json!("hi"));
        } else {
            panic!();
        }
    }

    #[test]
    fn handles_explicit_patch_operation() {
        let mut d = SseDecoder::new();
        d.feed(
            b"event: delta\ndata: {\"o\":\"patch\",\"v\":[{\"p\":\"/a\",\"o\":\"append\",\"v\":\"x\"},{\"p\":\"/b\",\"o\":\"replace\",\"v\":1}]}\n\n",
        );
        let ev = d.next_event().unwrap();
        if let Event::Delta(delta) = ev {
            assert_eq!(delta.patches.len(), 2);
        } else {
            panic!();
        }
    }
}
