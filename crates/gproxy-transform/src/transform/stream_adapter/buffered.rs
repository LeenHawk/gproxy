use super::{SseDecoder, SseTransformer};
use crate::protocol::ContentGenerationKind;

/// Convert a fully-buffered SSE body.
pub fn convert_buffered(mut transformer: SseTransformer, body: &[u8]) -> Vec<u8> {
    let mut out = transformer.push(body);
    out.extend(transformer.finish());
    out
}

/// Collapse a provider SSE stream into one response JSON of the same wire kind.
pub fn aggregate_buffered(kind: ContentGenerationKind, sse_body: &[u8]) -> Vec<u8> {
    use crate::transform::generate_content::stream_to_response as s2r;
    use ContentGenerationKind as K;

    let mut decoder = SseDecoder::new();
    let mut frames = decoder.push(sse_body);
    if let Some(tail) = decoder.finish() {
        frames.push(tail);
    }
    let datas: Vec<String> = frames
        .into_iter()
        .map(|frame| frame.data)
        .filter(|data| data.trim() != "[DONE]")
        .collect();

    macro_rules! collapse {
        ($ty:ty, $aggregate:path) => {{
            let events = datas
                .iter()
                .filter_map(|data| serde_json::from_str::<$ty>(data).ok());
            serde_json::to_vec(&$aggregate(events))
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
    };
    out.unwrap_or_else(|_| sse_body.to_vec())
}
