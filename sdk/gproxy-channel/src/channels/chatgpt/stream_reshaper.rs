//! Streaming reshape: `/f/conversation` SSE v1 → OpenAI
//! `chat.completion.chunk` SSE.
//!
//! This is the stream-mode counterpart of [`super::sse_to_openai::collect_all`]
//! — it feeds bytes in chunks and emits ready-to-forward `data: {...}\n\n`
//! lines as soon as each chunk produces enough state to render. The
//! engine plugs one of these in via [`crate::channel::StreamReshaper`].

use super::sse_to_openai::{OpenAiChunk, SseToOpenAi};
use super::sse_v1::SseDecoder;
use crate::channel::StreamReshaper;

/// Stream-wrapping reshaper. Owns a decoder + converter pair per request.
pub struct OpenAiChunkReshaper {
    decoder: SseDecoder,
    converter: SseToOpenAi,
    /// Whether we've already emitted the `data: [DONE]\n\n` terminator.
    done_emitted: bool,
}

impl OpenAiChunkReshaper {
    pub fn new(model: &str) -> Self {
        Self {
            decoder: SseDecoder::new(),
            converter: SseToOpenAi::with_model(model),
            done_emitted: false,
        }
    }

    fn drain_to_sse(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let events: Vec<_> = self.decoder.drain().collect();
        for event in events {
            let chunks = self.converter.on_event(event);
            append_openai_chunks(&mut out, &chunks);
            if self.converter.finished() && !self.done_emitted {
                out.extend_from_slice(b"data: [DONE]\n\n");
                self.done_emitted = true;
            }
        }
        out
    }
}

impl StreamReshaper for OpenAiChunkReshaper {
    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.decoder.feed(chunk);
        self.drain_to_sse()
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = self.drain_to_sse();
        if !self.done_emitted {
            // Upstream ended without a terminal marker — emit our own.
            let chunks = self.converter.on_event(super::sse_v1::Event::Done);
            append_openai_chunks(&mut out, &chunks);
            out.extend_from_slice(b"data: [DONE]\n\n");
            self.done_emitted = true;
        }
        out
    }
}

fn append_openai_chunks(out: &mut Vec<u8>, chunks: &[OpenAiChunk]) {
    for chunk in chunks {
        out.extend_from_slice(b"data: ");
        let s = serde_json::to_vec(chunk).unwrap_or_default();
        out.extend_from_slice(&s);
        out.extend_from_slice(b"\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeds_chunked_input_produces_openai_sse_data_frames() {
        let bytes = include_bytes!(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/samples/05_sse_response_text.txt"
            )
        );

        let mut reshaper = OpenAiChunkReshaper::new("gpt-5");

        // Feed the body in arbitrary chunks to emulate streaming.
        let mut out = Vec::<u8>::new();
        for split in bytes.chunks(512) {
            out.extend_from_slice(&reshaper.push_chunk(split));
        }
        out.extend_from_slice(&reshaper.finish());

        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("data: {"));
        assert!(text.ends_with("data: [DONE]\n\n"));

        // Reassemble the concatenated `delta.content` should reproduce
        // the assistant reply (bubble sort Chinese text).
        let mut reassembled = String::new();
        for line in text.lines() {
            let Some(payload) = line.strip_prefix("data: ") else {
                continue;
            };
            if payload == "[DONE]" {
                continue;
            }
            let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(payload) else {
                continue;
            };
            if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
                reassembled.push_str(s);
            }
        }
        assert!(
            reassembled.contains("冒泡") || reassembled.contains("bubble"),
            "reassembled: {}",
            reassembled.chars().take(80).collect::<String>()
        );
    }
}
