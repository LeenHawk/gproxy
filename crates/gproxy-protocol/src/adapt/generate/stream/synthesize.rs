//! Encode an already converted complete response as a native client lifecycle.
//! No new client identity is allocated after buffered invocation state was saved.

use super::super::GenerationOutcome;
use super::{
    event::{EventLimits, NativeEvent},
    output,
    reader::SourceFraming,
};
use crate::{
    Dialect, HttpBody, WireResponse,
    codec::CodecLimits,
    transform::{
        Converted, Report, TransformError,
        generate::stream as native,
        identity::{IdNamespace, IdentityFlow},
    },
    wire::{
        DeclaredFields,
        claude::{generate_content as c, stream as cs},
        gemini as g,
        openai::{
            chat::{self as h, stream as hs},
            responses::{self as r, stream as rs},
        },
    },
};
use bytes::Bytes;

mod sealed {
    pub trait Complete {}
}

pub trait CompleteResponse:
    sealed::Complete + DeclaredFields + serde::Serialize + Clone + Sized
{
    type Event: NativeEvent<Full = Self>;
    fn events(
        self,
        namespace: IdNamespace,
        limits: EventLimits,
    ) -> Result<Converted<Vec<Self::Event>>, TransformError>;
}

pub enum GenerationStreamOutcome {
    Success {
        response: WireResponse<HttpBody>,
        report: Report,
    },
    Rejected(WireResponse<Bytes>),
}

/// The buffered adapter must have completed its required state writes first.
/// This helper preserves all its client IDs and pre-encodes under the supplied
/// aggregate byte limit before exposing any stream bytes.
pub fn synthesize<C: CompleteResponse>(
    outcome: GenerationOutcome<C>,
    namespace: IdNamespace,
    framing: SourceFraming,
    codec: CodecLimits,
    events: EventLimits,
) -> Result<GenerationStreamOutcome, TransformError> {
    let (response, mut report) = match outcome {
        GenerationOutcome::Success { response, report } => (response, report),
        GenerationOutcome::Rejected(raw) => return Ok(GenerationStreamOutcome::Rejected(raw)),
    };
    if C::Event::DIALECT != Dialect::Gemini && framing != SourceFraming::Sse {
        return Err(TransformError::unsupported(
            "generation.synthesis.framing",
            "this native dialect requires SSE",
        ));
    }
    let converted = response.body.into_declared().events(namespace, events)?;
    let mut encoder = output::Encoder::new(framing, codec);
    let mut chunks = Vec::new();
    for event in converted.value {
        chunks.push(encoder.event(&event, codec)?);
    }
    let tail = encoder.finish::<C::Event>()?;
    if !tail.is_empty() {
        chunks.push(tail);
    }
    report.diagnostics.extend(converted.report.diagnostics);
    Ok(GenerationStreamOutcome::Success {
        response: WireResponse {
            status: response.status,
            headers: output::headers(response.headers, framing),
            body: HttpBody::Stream(Box::pin(futures_util::stream::iter(
                chunks.into_iter().map(Ok),
            ))),
        },
        report,
    })
}

/// Synthesize the complete Chat result using the actual client's usage option.
/// The generic `synthesize` utility instead preserves every field of its DTO.
pub fn synthesize_chat(
    mut outcome: GenerationOutcome<h::GenerateContentResponseBody>,
    request: &h::GenerateContentRequestBody,
    namespace: IdNamespace,
    framing: SourceFraming,
    codec: CodecLimits,
    events: EventLimits,
) -> Result<GenerationStreamOutcome, TransformError> {
    if request.stream.flatten() == Some(true)
        && request
            .stream_options
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(|options| options.include_usage)
            != Some(true)
        && let GenerationOutcome::Success { response, .. } = &mut outcome
    {
        response.body.usage = None;
    }
    synthesize(outcome, namespace, framing, codec, events)
}

impl sealed::Complete for c::GenerateContentResponseBody {}
impl CompleteResponse for c::GenerateContentResponseBody {
    type Event = cs::StreamEvent;
    fn events(
        self,
        _: IdNamespace,
        limits: EventLimits,
    ) -> Result<Converted<Vec<Self::Event>>, TransformError> {
        native::claude::synthesize_claude_stream(
            self,
            native::claude::ClaudeStreamLimits {
                max_json_bytes: limits.max_bytes,
                max_text_bytes: limits.max_bytes,
            },
        )
    }
}

impl sealed::Complete for h::GenerateContentResponseBody {}
impl CompleteResponse for h::GenerateContentResponseBody {
    type Event = hs::ChatCompletionChunk;
    fn events(
        self,
        _: IdNamespace,
        limits: EventLimits,
    ) -> Result<Converted<Vec<Self::Event>>, TransformError> {
        native::chat::synthesize_chat_stream(
            self,
            native::chat::ChatStreamLimits {
                max_bytes: limits.max_bytes,
            },
        )
    }
}

impl sealed::Complete for g::GenerateContentResponseBody {}
impl CompleteResponse for g::GenerateContentResponseBody {
    type Event = Self;
    fn events(
        self,
        _: IdNamespace,
        limits: EventLimits,
    ) -> Result<Converted<Vec<Self::Event>>, TransformError> {
        native::gemini::synthesize_gemini_stream(
            self,
            native::gemini::GeminiStreamLimits {
                max_bytes: limits.max_bytes,
            },
        )
    }
}

impl sealed::Complete for r::GenerateContentResponseBody {}
impl CompleteResponse for r::GenerateContentResponseBody {
    type Event = rs::StreamEvent;
    fn events(
        self,
        namespace: IdNamespace,
        limits: EventLimits,
    ) -> Result<Converted<Vec<Self::Event>>, TransformError> {
        for item in &self.output {
            let id = match item {
                r::ResponseOutputItem::FunctionCall(v) => &v.id,
                r::ResponseOutputItem::CustomToolCall(v) => &v.id,
                _ => continue,
            };
            if id.as_ref().is_none_or(|v| v.is_empty()) {
                return Err(super::missing(
                    "complete Responses output needs persisted item IDs before stream synthesis",
                ));
            }
        }
        native::responses::synthesize_responses_stream(
            self,
            &mut IdentityFlow::new(namespace),
            native::responses::ResponsesStreamLimits {
                max_bytes: limits.max_bytes,

                max_text_bytes: limits.max_bytes,
                max_json_bytes: limits.max_bytes,
            },
        )
    }
}
