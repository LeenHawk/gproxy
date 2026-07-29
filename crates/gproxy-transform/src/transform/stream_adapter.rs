//! Runtime SSE adaptation for cross-protocol content-generation streams.

mod buffered;
mod responses;
mod synthesize;

use super::common::sse::{SseDecoder, SseFrame};
use super::{TransformContext, TransformPair, dispatch};
use crate::protocol::ContentGenerationKind;
use crate::protocol::openai::ResponseStreamEvent;

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
                encode_responses_event(&event, &mut out);
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
        match dispatch::stream_event(self.pair, &self.ctx, &frame.data) {
            Ok(dispatch::StreamEventOut::Encoded { event, data }) => {
                encode_frame(self.inbound, event.as_deref(), &data, out);
            }
            Ok(dispatch::StreamEventOut::Responses(event)) => {
                if let Some(responses) = self.responses.as_mut() {
                    for event in responses.push(*event) {
                        encode_responses_event(&event, out);
                    }
                } else {
                    // Defensive: Responses events only occur with a Responses
                    // inbound, where the aggregation state is always present.
                    encode_responses_event(&event, out);
                }
            }
            Err(_) => self.skipped += 1,
        }
    }
}

/// Encode one converted event in the inbound wire format. Claude and Responses
/// inbound streams carry named SSE events (missing names fall back to
/// "message", as before the typed path); chat and Gemini are data-only.
fn encode_frame(kind: ContentGenerationKind, event: Option<&str>, data: &str, out: &mut Vec<u8>) {
    use ContentGenerationKind as K;
    let frame = match kind {
        K::ClaudeMessages | K::OpenAiResponses | K::OpenAiResponsesWebSocket => {
            SseFrame::event(event.unwrap_or("message"), data)
        }
        K::OpenAiChatCompletions | K::GeminiGenerateContent => SseFrame::data(data),
    };
    out.extend_from_slice(frame.encode().as_bytes());
}

/// Serialize + encode one Responses event as a named SSE frame.
fn encode_responses_event(event: &ResponseStreamEvent, out: &mut Vec<u8>) {
    let Ok(data) = serde_json::to_string(event) else {
        return;
    };
    let frame = SseFrame::event(event.event_name().unwrap_or("message"), data);
    out.extend_from_slice(frame.encode().as_bytes());
}

#[cfg(test)]
mod tests;
