//! Runtime SSE adaptation for cross-protocol content-generation streams.

mod buffered;
mod responses;
mod synthesize;

use super::common::sse::{SseDecoder, SseFrame, SseLimits};
use super::{
    TransformContext, TransformDiagnostic, TransformError, TransformOutput, TransformPair, dispatch,
};
use crate::protocol::openai::ResponseStreamEvent;
use crate::protocol::{ContentGenerationKind, OperationKind};
use serde::Deserialize;

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
    semantic_diagnostics: Vec<TransformDiagnostic>,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamDiagnostics {
    pub skipped_frames: u64,
    pub semantic_diagnostics: Vec<TransformDiagnostic>,
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
        let OperationKind::ContentGeneration(source) = ctx.source.kind() else {
            return Err(TransformError::InvalidInput {
                reason: "stream source is not a content-generation operation".to_owned(),
            });
        };
        let OperationKind::ContentGeneration(inbound) = ctx.target.kind() else {
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
            semantic_diagnostics: Vec::new(),
        })
    }

    /// Feed one upstream chunk; returns encoded inbound bytes (possibly empty).
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, TransformError> {
        Ok(self.push_detailed(chunk)?.value)
    }

    /// Feed one chunk and return semantic diagnostics produced by its events.
    pub fn push_detailed(
        &mut self,
        chunk: &[u8],
    ) -> Result<TransformOutput<Vec<u8>>, TransformError> {
        let diagnostic_start = self.semantic_diagnostics.len();
        let value = self.push_value(chunk)?;
        Ok(TransformOutput::new(
            value,
            self.semantic_diagnostics[diagnostic_start..].to_vec(),
        ))
    }

    fn push_value(&mut self, chunk: &[u8]) -> Result<Vec<u8>, TransformError> {
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
            .push(chunk)
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
        Ok(self.finish_detailed()?.value)
    }

    /// Flush the stream and return final semantic diagnostics.
    pub fn finish_detailed(&mut self) -> Result<TransformOutput<Vec<u8>>, TransformError> {
        let diagnostic_start = self.semantic_diagnostics.len();
        let value = self.finish_value()?;
        Ok(TransformOutput::new(
            value,
            self.semantic_diagnostics[diagnostic_start..].to_vec(),
        ))
    }

    fn finish_value(&mut self) -> Result<Vec<u8>, TransformError> {
        if self.finished {
            return Ok(Vec::new());
        }
        if self.failed {
            return Err(TransformError::InvalidInput {
                reason: "cannot finish a stream after a conversion error".to_owned(),
            });
        }
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish().inspect_err(|_| self.failed = true)?
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
        let converted = self.converter.finish_detailed()?;
        self.semantic_diagnostics.extend(converted.diagnostics);
        for event in converted.value {
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
            semantic_diagnostics: self.semantic_diagnostics.clone(),
        }
    }

    fn convert_into(&mut self, frame: SseFrame, out: &mut Vec<u8>) -> Result<(), TransformError> {
        if frame.data.trim() == "[DONE]" {
            self.terminal_seen = true;
            return Ok(());
        }
        let events = match self.converter.push_detailed_with_status(&frame.data) {
            Ok(events) => events,
            Err(_) if self.error_mode == StreamErrorMode::SkipInvalid => {
                // Preserve the old tolerant-stream behavior for a recognizable
                // terminal envelope whose full typed body has evolved beyond
                // what this build understands. This cold error path still uses
                // small typed envelopes rather than a dynamic Value tree.
                self.terminal_seen |= terminal_hint(self.source, &frame.data);
                self.skipped += 1;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        self.terminal_seen |= events.value.terminal;
        self.semantic_diagnostics.extend(events.diagnostics);
        for event in events.value.events {
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

fn terminal_hint(kind: ContentGenerationKind, data: &str) -> bool {
    #[derive(Deserialize)]
    struct TaggedEnvelope {
        #[serde(rename = "type")]
        type_: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeminiEnvelope {
        #[serde(default)]
        candidates: Vec<GeminiCandidateEnvelope>,
        prompt_feedback: Option<GeminiPromptFeedbackEnvelope>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeminiCandidateEnvelope {
        finish_reason: Option<serde::de::IgnoredAny>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeminiPromptFeedbackEnvelope {
        block_reason: Option<serde::de::IgnoredAny>,
    }

    match kind {
        ContentGenerationKind::OpenAiChatCompletions => false,
        ContentGenerationKind::ClaudeMessages => serde_json::from_str::<TaggedEnvelope>(data)
            .is_ok_and(|event| matches!(event.type_.as_str(), "message_stop" | "error")),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            serde_json::from_str::<TaggedEnvelope>(data).is_ok_and(|event| {
                matches!(
                    event.type_.as_str(),
                    "response.completed" | "response.incomplete" | "response.failed" | "error"
                )
            })
        }
        ContentGenerationKind::GeminiGenerateContent => {
            serde_json::from_str::<GeminiEnvelope>(data).is_ok_and(|event| {
                event
                    .candidates
                    .iter()
                    .any(|candidate| candidate.finish_reason.is_some())
                    || event
                        .prompt_feedback
                        .is_some_and(|feedback| feedback.block_reason.is_some())
            })
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
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
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
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
