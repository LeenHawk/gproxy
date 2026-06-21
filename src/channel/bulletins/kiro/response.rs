//! Kiro Smithy event-stream → OpenAI Responses SSE decoder.
//!
//! [`KiroStreamDecoder`] is a per-stream [`ChannelStreamDecoder`]: it feeds raw
//! upstream bytes into a [`SmithyFrameParser`] and, for each decoded frame,
//! dispatches by the `:event-type` header into the OpenAI Responses SSE
//! lifecycle:
//!
//! ```text
//!   assistantResponseEvent  → response.output_text.delta   (url-decode + dedup)
//!   reasoningContentEvent   → response.reasoning_text.delta (url-decode + dedup)
//!   metadataEvent           → tokenUsage  (captured for response.completed)
//!   messageMetadataEvent    → conversationId (captured as response id)
//!   *ServerException / invalidStateEvent → an SSE `error` event
//! ```
//!
//! On the FIRST emitted byte it opens the lifecycle (`response.created`); the
//! first content/reasoning delta opens its `output_item.added` +
//! `content_part.added`. [`finish`](ChannelStreamDecoder::finish) closes the
//! open items and emits `response.completed` (carrying usage). Ported from the
//! v1 `gproxy-channel` kiro impl; the decoder owns its state (the v1 keyed
//! `DashMap` is unnecessary — one decoder instance per stream). Fully sync, so
//! it compiles on the wasm edge target.
//!
//! **delta-dedup** ([`dedup_chunk`](super::sse::dedup_chunk)): Kiro sometimes
//! re-sends an accumulating prefix (`"he"` then `"hello"`); we diff against the
//! previous chunk and emit only the new suffix. **url-decode**
//! ([`url_decode`](super::sse::url_decode)): content arrives percent-encoded;
//! `%XX` triples are decoded to bytes.

use serde_json::{Value, json};

use crate::channel::ChannelStreamDecoder;

use super::smithy::{SmithyFrame, SmithyFrameParser};
use super::sse::{
    dedup_chunk, gen_id, message_item, openai_usage, push_sse, reasoning_item, url_decode,
};
use super::tool_calls::ToolCallTracker;

/// Per-stream Responses SSE state machine over the Smithy frame stream.
pub struct KiroStreamDecoder {
    parser: SmithyFrameParser,
    response_id: String,
    message_id: String,
    reasoning_id: String,
    model: String,
    content: String,
    reasoning: String,
    last_assistant: String,
    last_reasoning: String,
    usage: Option<Value>,
    initialized: bool,
    content_started: bool,
    reasoning_started: bool,
    tool_calls: ToolCallTracker,
    seq: u64,
}

impl KiroStreamDecoder {
    pub fn new() -> Self {
        Self {
            parser: SmithyFrameParser::new(),
            response_id: gen_id("resp"),
            message_id: gen_id("msg"),
            reasoning_id: gen_id("rs"),
            model: "kiro".to_string(),
            content: String::new(),
            reasoning: String::new(),
            last_assistant: String::new(),
            last_reasoning: String::new(),
            usage: None,
            initialized: false,
            content_started: false,
            reasoning_started: false,
            tool_calls: ToolCallTracker::default(),
            seq: 0,
        }
    }

    fn next_seq(&mut self) -> u64 {
        let s = self.seq;
        self.seq += 1;
        s
    }

    /// Emit `response.created` once, on first output.
    fn ensure_started(&mut self, out: &mut Vec<u8>) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let seq = self.next_seq();
        let body = self.response_body("in_progress", false);
        push_sse(
            out,
            json!({
                "type": "response.created",
                "sequence_number": seq,
                "response": body,
            }),
        );
    }

    /// Open the assistant message item + its first text part.
    fn ensure_content(&mut self, out: &mut Vec<u8>) {
        if self.content_started {
            return;
        }
        self.content_started = true;
        let seq = self.next_seq();
        push_sse(
            out,
            json!({
                "type": "response.output_item.added",
                "sequence_number": seq,
                "output_index": 0,
                "item": message_item(&self.message_id, "", "in_progress"),
            }),
        );
        let seq = self.next_seq();
        push_sse(
            out,
            json!({
                "type": "response.content_part.added",
                "sequence_number": seq,
                "output_index": 0,
                "item_id": self.message_id,
                "content_index": 0,
                "part": { "type": "output_text", "text": "", "annotations": [] },
            }),
        );
    }

    /// Open the reasoning item.
    fn ensure_reasoning(&mut self, out: &mut Vec<u8>) {
        if self.reasoning_started {
            return;
        }
        self.reasoning_started = true;
        let seq = self.next_seq();
        push_sse(
            out,
            json!({
                "type": "response.output_item.added",
                "sequence_number": seq,
                "output_index": 1,
                "item": reasoning_item(&self.reasoning_id, "", "in_progress"),
            }),
        );
    }

    /// Dispatch one decoded frame into SSE output.
    fn handle_frame(&mut self, frame: SmithyFrame, out: &mut Vec<u8>) {
        let Some(event_type) = frame.event_type.as_deref() else {
            return;
        };
        // Kiro sometimes nests the payload under the event-type key.
        let payload = frame
            .payload
            .get(event_type)
            .unwrap_or(&frame.payload)
            .clone();

        match event_type {
            "assistantResponseEvent" => {
                if let Some(text) = payload.get("content").and_then(Value::as_str) {
                    let delta = url_decode(&dedup_chunk(text, &mut self.last_assistant));
                    if !delta.is_empty() {
                        self.ensure_content(out);
                        self.content.push_str(&delta);
                        let seq = self.next_seq();
                        push_sse(
                            out,
                            json!({
                                "type": "response.output_text.delta",
                                "sequence_number": seq,
                                "output_index": 0,
                                "item_id": self.message_id,
                                "content_index": 0,
                                "delta": delta,
                            }),
                        );
                    }
                }
                if self.model == "kiro"
                    && let Some(m) = payload.get("modelId").and_then(Value::as_str)
                {
                    self.model = m.to_string();
                }
            }
            "reasoningContentEvent" => {
                if let Some(text) = payload
                    .get("text")
                    .or_else(|| payload.get("content"))
                    .and_then(Value::as_str)
                {
                    let delta = url_decode(&dedup_chunk(text, &mut self.last_reasoning));
                    if !delta.is_empty() {
                        self.ensure_reasoning(out);
                        self.reasoning.push_str(&delta);
                        let seq = self.next_seq();
                        push_sse(
                            out,
                            json!({
                                "type": "response.reasoning_text.delta",
                                "sequence_number": seq,
                                "output_index": 1,
                                "item_id": self.reasoning_id,
                                "content_index": 0,
                                "delta": delta,
                            }),
                        );
                    }
                }
            }
            "messageMetadataEvent" => {
                if self.response_id.starts_with("resp_")
                    && let Some(id) = payload.get("conversationId").and_then(Value::as_str)
                {
                    self.response_id = id.to_string();
                }
            }
            "metadataEvent" => {
                if self.usage.is_none()
                    && let Some(usage) = payload.get("tokenUsage").cloned()
                {
                    self.usage = Some(usage);
                }
            }
            "toolUseEvent" => {
                self.tool_calls.handle(&payload, &mut self.seq, out);
            }
            "invalidStateEvent" | "InternalServerException" | "internalServerException" => {
                let message = payload
                    .get("message")
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("reason").and_then(Value::as_str))
                    .unwrap_or("kiro upstream stream error")
                    .to_string();
                let seq = self.next_seq();
                push_sse(
                    out,
                    json!({
                        "type": "error",
                        "sequence_number": seq,
                        "error": {
                            "type": "kiro_error",
                            "code": "kiro_eventstream_error",
                            "message": message,
                        },
                    }),
                );
            }
            _ => {}
        }
    }

    /// Build the `response` object for `response.created` / `response.completed`.
    fn response_body(&self, status: &str, include_output: bool) -> Value {
        let mut output = Vec::new();
        if include_output && self.reasoning_started {
            output.push(reasoning_item(
                &self.reasoning_id,
                &self.reasoning,
                "completed",
            ));
        }
        if include_output && self.content_started {
            output.push(message_item(&self.message_id, &self.content, "completed"));
        }
        if include_output {
            output.extend(self.tool_calls.completed_items());
        }
        let mut response = json!({
            "id": self.response_id,
            "created_at": crate::util::time::unix_now(),
            "metadata": {},
            "model": self.model,
            "object": "response",
            "output": output,
            "parallel_tool_calls": false,
            "temperature": 1.0,
            "tool_choice": "auto",
            "tools": [],
            "top_p": 1.0,
            "output_text": self.content,
            "status": status,
        });
        if include_output
            && let Some(usage) = self.usage.clone().and_then(openai_usage)
            && let Some(obj) = response.as_object_mut()
        {
            obj.insert("usage".into(), usage);
        }
        response
    }
}

impl Default for KiroStreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelStreamDecoder for KiroStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let frames = self.parser.push(chunk);
        let mut out = Vec::new();
        if !frames.is_empty() {
            self.ensure_started(&mut out);
        }
        for frame in frames {
            self.handle_frame(frame, &mut out);
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.ensure_started(&mut out);
        if self.reasoning_started {
            let seq = self.next_seq();
            push_sse(
                &mut out,
                json!({
                    "type": "response.reasoning_text.done",
                    "sequence_number": seq,
                    "output_index": 1,
                    "item_id": self.reasoning_id,
                    "content_index": 0,
                    "text": self.reasoning,
                }),
            );
            let seq = self.next_seq();
            push_sse(
                &mut out,
                json!({
                    "type": "response.output_item.done",
                    "sequence_number": seq,
                    "output_index": 1,
                    "item": reasoning_item(&self.reasoning_id, &self.reasoning, "completed"),
                }),
            );
        }
        if self.content_started {
            let seq = self.next_seq();
            push_sse(
                &mut out,
                json!({
                    "type": "response.output_text.done",
                    "sequence_number": seq,
                    "output_index": 0,
                    "item_id": self.message_id,
                    "content_index": 0,
                    "text": self.content,
                }),
            );
            let seq = self.next_seq();
            push_sse(
                &mut out,
                json!({
                    "type": "response.content_part.done",
                    "sequence_number": seq,
                    "output_index": 0,
                    "item_id": self.message_id,
                    "content_index": 0,
                    "part": { "type": "output_text", "text": self.content, "annotations": [] },
                }),
            );
            let seq = self.next_seq();
            push_sse(
                &mut out,
                json!({
                    "type": "response.output_item.done",
                    "sequence_number": seq,
                    "output_index": 0,
                    "item": message_item(&self.message_id, &self.content, "completed"),
                }),
            );
        }
        // Close any tool call whose `stop` frame never arrived.
        self.tool_calls.finish(&mut self.seq, &mut out);
        let seq = self.next_seq();
        let body = self.response_body("completed", true);
        push_sse(
            &mut out,
            json!({
                "type": "response.completed",
                "sequence_number": seq,
                "response": body,
            }),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::smithy::build_frame;
    use super::*;

    /// Concatenate all `data:` payloads emitted by a decoder run for assertions.
    fn sse_text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn assistant_event_to_sse() {
        // One assistantResponseEvent frame → output_text.delta carrying "hi",
        // wrapped in the Responses lifecycle (created → item.added → delta).
        let mut dec = KiroStreamDecoder::new();
        let frame = build_frame("assistantResponseEvent", br#"{"content":"hi"}"#);
        let out = sse_text(&dec.push(&frame));
        assert!(out.contains("response.created"));
        assert!(out.contains("response.output_text.delta"));
        assert!(out.contains(r#""delta":"hi""#));

        let fin = sse_text(&dec.finish());
        assert!(fin.contains("response.output_text.done"));
        assert!(fin.contains("response.completed"));

        // delta-dedup: accumulating chunks "he" then "hello" emit "he" then
        // "llo" (only the new suffix), never the re-sent prefix.
        let mut dec = KiroStreamDecoder::new();
        let first = sse_text(&dec.push(&build_frame(
            "assistantResponseEvent",
            br#"{"content":"he"}"#,
        )));
        assert!(first.contains(r#""delta":"he""#));
        let second = sse_text(&dec.push(&build_frame(
            "assistantResponseEvent",
            br#"{"content":"hello"}"#,
        )));
        assert!(second.contains(r#""delta":"llo""#));
        assert!(!second.contains(r#""delta":"hello""#));
    }

    #[test]
    fn tool_use_events_to_function_call_sse() {
        // Two tool calls, each streamed as partial-then-stop fragments, plus a
        // text frame first so output indices land at 2 and 3.
        let mut dec = KiroStreamDecoder::new();
        dec.push(&build_frame(
            "assistantResponseEvent",
            br#"{"content":"hi"}"#,
        ));

        // Call A: name on the first (partial) fragment, args split across two.
        let a1 = sse_text(&dec.push(&build_frame(
            "toolUseEvent",
            br#"{"name":"get_weather","toolUseId":"call_a","input":"{\"city\":","stop":false}"#,
        )));
        assert!(a1.contains("response.output_item.added"));
        assert!(a1.contains(r#""type":"function_call""#));
        assert!(a1.contains(r#""call_id":"call_a""#));
        assert!(a1.contains(r#""name":"get_weather""#));
        assert!(a1.contains("response.function_call_arguments.delta"));
        assert!(a1.contains(r#""output_index":2"#));

        let a2 = sse_text(&dec.push(&build_frame(
            "toolUseEvent",
            br#"{"name":"get_weather","toolUseId":"call_a","input":"\"NYC\"}","stop":true}"#,
        )));
        assert!(a2.contains("response.function_call_arguments.done"));
        assert!(a2.contains(r#""arguments":"{\"city\":\"NYC\"}""#));
        assert!(a2.contains("response.output_item.done"));

        // Call B: a distinct toolUseId → its own item at the next output_index.
        let b = sse_text(&dec.push(&build_frame(
            "toolUseEvent",
            br#"{"name":"lookup","toolUseId":"call_b","input":"{}","stop":true}"#,
        )));
        assert!(b.contains(r#""call_id":"call_b""#));
        assert!(b.contains(r#""output_index":3"#));
        assert!(b.contains("response.function_call_arguments.done"));

        let fin = sse_text(&dec.finish());
        assert!(fin.contains("response.completed"));
        // Both completed function-call items appear in the final output array.
        assert!(fin.contains(r#""call_id":"call_a""#));
        assert!(fin.contains(r#""call_id":"call_b""#));
    }

    #[test]
    fn tool_use_without_stop_is_closed_on_finish() {
        // A tool call whose `stop` frame never arrives must still be closed by
        // finish() so the Responses stream is well-formed.
        let mut dec = KiroStreamDecoder::new();
        dec.push(&build_frame(
            "toolUseEvent",
            br#"{"name":"f","toolUseId":"c1","input":"{\"a\":1}","stop":false}"#,
        ));
        let fin = sse_text(&dec.finish());
        assert!(fin.contains("response.function_call_arguments.done"));
        assert!(fin.contains(r#""arguments":"{\"a\":1}""#));
        assert!(fin.contains("response.output_item.done"));
        assert!(fin.contains("response.completed"));
    }
}
