//! Runtime SSE adaptation for cross-protocol content-generation streams.

mod buffered;
mod responses;
mod synthesize;

use serde_json::Value;

use super::common::sse::{SseDecoder, SseFrame};
use super::{TransformContext, TransformPair, dispatch};
use crate::protocol::ContentGenerationKind;

use responses::ResponsesStreamState;

pub use buffered::{aggregate_buffered, convert_buffered};
pub use responses::ResponsesStreamNormalizer;
pub use synthesize::synthesize_sse;

pub struct SseTransformer {
    decoder: SseDecoder,
    /// Reverse pair: upstream kind to inbound kind.
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
        if frame.data.trim() == "[DONE]" {
            return;
        }
        let event: Value = match serde_json::from_str(&frame.data) {
            Ok(value) => value,
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
            Err(_) => self.skipped += 1,
        }
    }
}

/// Encode one converted event in the inbound wire format.
fn encode_frame(kind: ContentGenerationKind, value: &Value) -> String {
    use ContentGenerationKind as K;
    let data = value.to_string();
    match kind {
        K::ClaudeMessages | K::OpenAiResponses | K::OpenAiResponsesWebSocket => {
            let name = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("message");
            SseFrame::event(name, data).encode()
        }
        K::OpenAiChatCompletions | K::GeminiGenerateContent => SseFrame::data(data).encode(),
    }
}

#[cfg(test)]
mod tests;
