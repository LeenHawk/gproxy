use super::{SseDecoder, SseTransformer};
use crate::protocol::ContentGenerationKind;
use crate::transform::TransformError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BufferedDiagnostics {
    pub decoded_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedAggregation {
    pub body: Vec<u8>,
    pub diagnostics: BufferedDiagnostics,
}

/// Convert a fully-buffered SSE body.
pub fn convert_buffered(
    mut transformer: SseTransformer,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    let mut out = transformer.push(body)?;
    out.extend(transformer.finish()?);
    Ok(out)
}

/// Collapse a provider SSE stream into one response JSON of the same wire kind.
pub fn aggregate_buffered(
    kind: ContentGenerationKind,
    sse_body: &[u8],
) -> Result<BufferedAggregation, TransformError> {
    use crate::transform::generate_content::stream_to_response as s2r;
    use ContentGenerationKind as K;

    let mut decoder = SseDecoder::new();
    let mut frames = decoder.push(sse_body)?;
    if let Some(tail) = decoder.finish()? {
        frames.push(tail);
    }
    let decoded_frames = frames.len();
    let datas: Vec<String> = frames
        .into_iter()
        .map(|frame| frame.data)
        .filter(|data| data.trim() != "[DONE]")
        .collect();

    macro_rules! collapse {
        ($ty:ty, $aggregate:path) => {{
            let events = datas
                .iter()
                .enumerate()
                .map(|(index, data)| {
                    serde_json::from_str::<$ty>(data).map_err(|error| {
                        TransformError::InvalidInput {
                            reason: format!("decode buffered stream frame {index}: {error}"),
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::to_vec(&$aggregate(events.into_iter())).map_err(|error| {
                TransformError::Serialization {
                    reason: error.to_string(),
                }
            })
        }};
    }

    let out = match kind {
        K::OpenAiResponses | K::OpenAiResponsesWebSocket => collapse!(
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
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }?;
    Ok(BufferedAggregation {
        body: out,
        diagnostics: BufferedDiagnostics { decoded_frames },
    })
}
