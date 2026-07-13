//! Normalize Claude Web SSE and synthesize the usage fields omitted by the web
//! endpoint. Modern Messages events are retained; legacy `{completion}` deltas
//! are converted to Messages SSE.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::channel::ChannelStreamDecoder;
use crate::transform::common::{SseDecoder, SseFrame};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Unknown,
    Modern,
    Legacy,
}

pub(super) struct ClaudeWebStreamDecoder {
    decoder: SseDecoder,
    kind: StreamKind,
    legacy_started: bool,
    message_id: String,
    input_tokens: u64,
    output_text: String,
    output_tokens: Arc<AtomicU64>,
    tool_use_index: Option<u64>,
    skipped_tool_result_index: Option<u64>,
}

impl ClaudeWebStreamDecoder {
    pub(super) fn new(input_tokens: u64, output_tokens: Arc<AtomicU64>) -> Self {
        Self {
            decoder: SseDecoder::new(),
            kind: StreamKind::Unknown,
            legacy_started: false,
            message_id: format!("msg_{}", crate::util::rand::uuid_v4().replace('-', "")),
            input_tokens,
            output_text: String::new(),
            output_tokens,
            tool_use_index: None,
            skipped_tool_result_index: None,
        }
    }

    fn frame(&mut self, frame: SseFrame, out: &mut Vec<u8>) {
        let Ok(mut value) = serde_json::from_str::<Value>(&frame.data) else {
            return;
        };
        if let Some(kind) = value.get("type").and_then(Value::as_str).map(str::to_owned) {
            self.kind = StreamKind::Modern;
            self.modern_frame(&kind, &mut value, out);
            return;
        }
        let Some(delta) = value
            .get("completion")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        if self.kind == StreamKind::Modern {
            return;
        }
        self.kind = StreamKind::Legacy;
        if !self.legacy_started {
            self.legacy_started = true;
            self.push_event(
                out,
                json!({
                    "type":"message_start",
                    "message":{
                        "id":self.message_id,
                        "type":"message",
                        "role":"assistant",
                        "content":[],
                        "model":"claude-web",
                        "stop_reason":null,
                        "stop_sequence":null,
                        "usage":{"input_tokens":self.input_tokens,"output_tokens":0}
                    }
                }),
            );
            self.push_event(
                out,
                json!({
                    "type":"content_block_start",
                    "index":0,
                    "content_block":{"type":"text","text":""}
                }),
            );
        }
        if !delta.is_empty() {
            self.add_output_text(&delta);
            self.push_event(
                out,
                json!({
                    "type":"content_block_delta",
                    "index":0,
                    "delta":{"type":"text_delta","text":delta}
                }),
            );
        }
    }

    fn modern_frame(&mut self, kind: &str, value: &mut Value, out: &mut Vec<u8>) {
        let index = value.get("index").and_then(Value::as_u64);
        if kind == "content_block_start" {
            let block_type = value
                .get("content_block")
                .and_then(|block| block.get("type"))
                .and_then(Value::as_str);
            if block_type == Some("tool_result") {
                self.skipped_tool_result_index = index;
                return;
            }
            if block_type == Some("tool_use") {
                self.tool_use_index = index;
            }
        }
        if self.skipped_tool_result_index.is_some() && self.skipped_tool_result_index == index {
            if kind == "content_block_stop" {
                self.skipped_tool_result_index = None;
            }
            return;
        }
        if kind == "content_block_delta" {
            self.count_delta(value);
        } else if kind == "message_start" {
            if let Some(message) = value.get_mut("message").and_then(Value::as_object_mut) {
                message.insert(
                    "usage".into(),
                    json!({"input_tokens":self.input_tokens,"output_tokens":0}),
                );
            }
        } else if kind == "message_delta" {
            value["usage"] = json!({"output_tokens":self.current_output_tokens()});
        }
        self.push_event(out, value.clone());
        if kind == "content_block_stop" && self.tool_use_index == index {
            self.tool_use_index = None;
            self.push_event(
                out,
                json!({
                    "type":"message_delta",
                    "delta":{"stop_reason":"tool_use","stop_sequence":null},
                    "usage":{"output_tokens":self.current_output_tokens()}
                }),
            );
            self.push_event(out, json!({"type":"message_stop"}));
        }
    }

    fn count_delta(&mut self, value: &Value) {
        let text = value.get("delta").and_then(|delta| {
            delta
                .get("text")
                .or_else(|| delta.get("thinking"))
                .or_else(|| delta.get("partial_json"))
                .and_then(Value::as_str)
        });
        if let Some(text) = text {
            self.add_output_text(text);
        }
    }

    fn add_output_text(&mut self, text: &str) {
        self.output_text.push_str(text);
        self.output_tokens
            .store(self.current_output_tokens(), Ordering::Relaxed);
    }

    fn current_output_tokens(&self) -> u64 {
        crate::tokenize::count_text(&self.output_text)
    }

    fn push_event(&self, out: &mut Vec<u8>, value: Value) {
        let event = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");
        out.extend_from_slice(
            SseFrame::event(event, value.to_string())
                .encode()
                .as_bytes(),
        );
    }

    fn finish_legacy(&self, out: &mut Vec<u8>) {
        if !self.legacy_started {
            return;
        }
        self.push_event(out, json!({"type":"content_block_stop","index":0}));
        self.push_event(
            out,
            json!({
                "type":"message_delta",
                "delta":{"stop_reason":"end_turn","stop_sequence":null},
                "usage":{"output_tokens":self.current_output_tokens()}
            }),
        );
        self.push_event(out, json!({"type":"message_stop"}));
    }
}

impl ChannelStreamDecoder for ClaudeWebStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(chunk) {
            self.frame(frame, &mut out);
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish() {
            self.frame(frame, &mut out);
        }
        if self.kind == StreamKind::Legacy {
            self.finish_legacy(&mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_completion_becomes_messages_sse_with_usage() {
        let counter = Arc::new(AtomicU64::new(0));
        let mut decoder = ClaudeWebStreamDecoder::new(7, Arc::clone(&counter));
        let mut out =
            decoder.push(b"data: {\"completion\":\"hel\"}\n\ndata: {\"completion\":\"lo\"}\n\n");
        out.extend(decoder.finish());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("event: message_start"));
        assert!(text.contains("\"text\":\"hel\""));
        assert!(text.contains("\"text\":\"lo\""));
        assert!(text.contains("\"input_tokens\":7"));
        assert_eq!(
            counter.load(Ordering::Relaxed),
            crate::tokenize::count_text("hello")
        );
        assert!(text.ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
    }

    #[test]
    fn modern_messages_events_remain_canonical() {
        let counter = Arc::new(AtomicU64::new(0));
        let mut decoder = ClaudeWebStreamDecoder::new(1, Arc::clone(&counter));
        let input = b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n";
        let out = decoder.push(input);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("event: content_block_delta"));
        assert!(text.contains("\"text\":\"hi\""));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn modern_tool_use_stops_and_resumed_tool_result_is_hidden() {
        let mut decoder = ClaudeWebStreamDecoder::new(9, Arc::new(AtomicU64::new(0)));
        let first = decoder.push(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        let text = String::from_utf8(first).unwrap();
        assert!(text.contains("\"stop_reason\":\"tool_use\""));
        assert!(text.contains("event: message_stop"));
        assert!(text.contains("\"input_tokens\":9"));

        let resumed = decoder.push(b"data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_1\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n");
        let text = String::from_utf8(resumed).unwrap();
        assert!(!text.contains("tool_result"));
        assert!(text.contains("\"type\":\"text\""));
    }
}
