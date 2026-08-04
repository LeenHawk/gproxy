//! Runtime SSE adaptation for cross-protocol content-generation streams.

mod buffered;
mod responses;
mod synthesize;

use super::common::sse::{SseDecoder, SseFrame, SseLimits};
use super::{TransformContext, TransformError, TransformPair, dispatch};
use crate::protocol::openai::ResponseStreamEvent;
use crate::protocol::{ContentGenerationKind, OperationKind};

use responses::ResponsesStreamState;

pub use buffered::{
    BufferedAggregation, BufferedDiagnostics, aggregate_buffered, convert_buffered,
};
pub use responses::ResponsesStreamNormalizer;
pub use synthesize::synthesize_sse;

pub struct SseTransformer {
    decoder: SseDecoder,
    converter: dispatch::StreamConverter,
    source: ContentGenerationKind,
    inbound: ContentGenerationKind,
    responses: Option<ResponsesStreamState>,
    error_mode: StreamErrorMode,
    require_terminal: bool,
    terminal_seen: bool,
    failed: bool,
    finished: bool,
    skipped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamErrorMode {
    #[default]
    Strict,
    SkipInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamOptions {
    pub limits: SseLimits,
    pub error_mode: StreamErrorMode,
    pub require_terminal: bool,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            limits: SseLimits::default(),
            error_mode: StreamErrorMode::Strict,
            require_terminal: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamDiagnostics {
    pub skipped_frames: u64,
}

impl SseTransformer {
    pub fn new(pair: TransformPair, ctx: TransformContext) -> Result<Self, TransformError> {
        Self::with_options(pair, ctx, StreamOptions::default())
    }

    pub fn with_options(
        pair: TransformPair,
        ctx: TransformContext,
        options: StreamOptions,
    ) -> Result<Self, TransformError> {
        let OperationKind::ContentGeneration(source) = ctx.source.kind else {
            return Err(TransformError::InvalidInput {
                reason: "stream source is not a content-generation operation".to_owned(),
            });
        };
        let OperationKind::ContentGeneration(inbound) = ctx.target.kind else {
            return Err(TransformError::InvalidInput {
                reason: "stream target is not a content-generation operation".to_owned(),
            });
        };
        Ok(Self {
            decoder: SseDecoder::with_limits(options.limits),
            converter: dispatch::StreamConverter::new(pair, ctx)?,
            source,
            inbound,
            responses: matches!(
                inbound,
                ContentGenerationKind::OpenAiResponses
                    | ContentGenerationKind::OpenAiResponsesWebSocket
            )
            .then(ResponsesStreamState::default),
            error_mode: options.error_mode,
            require_terminal: options.require_terminal,
            terminal_seen: false,
            failed: false,
            finished: false,
            skipped: 0,
        })
    }

    /// Feed one upstream chunk; returns encoded inbound bytes (possibly empty).
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, TransformError> {
        if self.finished {
            return Err(TransformError::InvalidInput {
                reason: "cannot push after stream finish".to_owned(),
            });
        }
        if self.failed {
            return Err(TransformError::InvalidInput {
                reason: "stream is failed after an earlier conversion error".to_owned(),
            });
        }
        let mut out = Vec::new();
        let frames = self
            .decoder
            .try_push(chunk)
            .inspect_err(|_| self.failed = true)?;
        for frame in frames {
            if let Err(error) = self.convert_into(frame, &mut out) {
                self.failed = true;
                return Err(error);
            }
        }
        Ok(out)
    }

    /// Flush the trailing frame and emit the inbound terminator.
    pub fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        if self.finished {
            return Ok(Vec::new());
        }
        if self.failed {
            return Err(TransformError::InvalidInput {
                reason: "cannot finish a stream after a conversion error".to_owned(),
            });
        }
        let mut out = Vec::new();
        if let Some(frame) = self
            .decoder
            .try_finish()
            .inspect_err(|_| self.failed = true)?
            && let Err(error) = self.convert_into(frame, &mut out)
        {
            self.failed = true;
            return Err(error);
        }
        if self.require_terminal && !self.terminal_seen {
            self.failed = true;
            return Err(TransformError::UnexpectedEof {
                reason: "upstream ended before a protocol terminal event",
            });
        }
        for event in self.converter.finish()? {
            self.encode_converted(event, &mut out)?;
        }
        if let Some(responses) = self.responses.as_mut() {
            for event in responses.finish() {
                encode_responses_event(&event, &mut out)?;
            }
        }
        if self.inbound == ContentGenerationKind::OpenAiChatCompletions {
            out.extend_from_slice(b"data: [DONE]\n\n");
        }
        self.finished = true;
        Ok(out)
    }

    pub fn diagnostics(&self) -> StreamDiagnostics {
        StreamDiagnostics {
            skipped_frames: self.skipped,
        }
    }

    fn convert_into(&mut self, frame: SseFrame, out: &mut Vec<u8>) -> Result<(), TransformError> {
        if frame.data.trim() == "[DONE]" {
            self.terminal_seen = true;
            return Ok(());
        }
        self.terminal_seen |= is_terminal_event(self.source, &frame.data);
        let events = match self.converter.push(&frame.data) {
            Ok(events) => events,
            Err(_) if self.error_mode == StreamErrorMode::SkipInvalid => {
                self.skipped += 1;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        for event in events {
            self.encode_converted(event, out)?;
        }
        Ok(())
    }

    fn encode_converted(
        &mut self,
        event: dispatch::StreamEventOut,
        out: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        match event {
            dispatch::StreamEventOut::Encoded { event, data } => {
                encode_frame(self.inbound, event.as_deref(), &data, out);
            }
            dispatch::StreamEventOut::Responses(event) => {
                if let Some(responses) = self.responses.as_mut() {
                    for event in responses.push(*event) {
                        encode_responses_event(&event, out)?;
                    }
                } else {
                    encode_responses_event(&event, out)?;
                }
            }
        }
        Ok(())
    }
}

fn is_terminal_event(kind: ContentGenerationKind, data: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return false;
    };
    match kind {
        ContentGenerationKind::OpenAiChatCompletions => false,
        ContentGenerationKind::ClaudeMessages => matches!(
            value.get("type").and_then(serde_json::Value::as_str),
            Some("message_stop" | "error")
        ),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => matches!(
            value.get("type").and_then(serde_json::Value::as_str),
            Some("response.completed" | "response.incomplete" | "response.failed" | "error")
        ),
        ContentGenerationKind::GeminiGenerateContent => {
            value
                .get("candidates")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|candidates| {
                    candidates.iter().any(|candidate| {
                        candidate
                            .get("finishReason")
                            .is_some_and(|reason| !reason.is_null())
                    })
                })
                || value
                    .pointer("/promptFeedback/blockReason")
                    .is_some_and(|reason| !reason.is_null())
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
fn encode_responses_event(
    event: &ResponseStreamEvent,
    out: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let data = serde_json::to_string(event).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })?;
    let frame = SseFrame::event(event.event_name().unwrap_or("message"), data);
    out.extend_from_slice(frame.encode().as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests;
