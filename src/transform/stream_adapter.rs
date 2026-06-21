//! Runtime SSE adapter for cross-protocol streaming: decode upstream frames,
//! convert each event through the resolved (reverse) pair, re-encode in the
//! inbound wire format. Sync core — shared by the native stream wrapper
//! (`pipeline/stream.rs`) and the buffered path (wasm / buffered attempts).
//!
//! Pair `stream_event` fns are 1:1 and stateless; any future cross-event
//! aggregation (block indexes, tool-call identity, final usage) lives HERE
//! (see transform/README.md).

use serde_json::Value;

use super::common::sse::{SseDecoder, SseFrame};
use super::{TransformContext, TransformPair, dispatch};
use crate::protocol::ContentGenerationKind;

pub struct SseTransformer {
    decoder: SseDecoder,
    /// Reverse pair: upstream kind → inbound kind.
    pair: TransformPair,
    ctx: TransformContext,
    inbound: ContentGenerationKind,
    skipped: u64,
}

impl SseTransformer {
    pub fn new(pair: TransformPair, ctx: TransformContext, inbound: ContentGenerationKind) -> Self {
        Self {
            decoder: SseDecoder::new(),
            pair,
            ctx,
            inbound,
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
                out.extend_from_slice(encode_frame(self.inbound, &converted).as_bytes())
            }
            Err(_) => {
                self.skipped += 1;
            }
        }
    }
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
