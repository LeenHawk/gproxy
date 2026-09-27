//! Unwrapping the Code Assist envelope out of a delivered response stream.
//!
//! The upstream answers `:streamGenerateContent?alt=sse` with SSE whose every
//! `data:` payload is `{"response": {...}}`; the channel declares Gemini as
//! its native dialect, so each payload is re-emitted as the bare Gemini
//! chunk before the host or the client ever sees it (v3
//! `shared/code_assist/stream.rs`, which did the same inside a
//! `StreamDecoder`). Framing and event ordering are preserved; only the JSON
//! nesting changes.

use super::{invalid_response, normalize_content, unwrap_value};
use crate::channel::ChannelError;
use futures_util::{StreamExt, stream};
use gproxy_protocol::HttpBody;
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseEncoder, SseEvent, SseFrame};
use gproxy_protocol::connection::{ByteStream, Bytes, TransportError};
use serde_json::Value;
use std::collections::VecDeque;

/// Bounds for re-framing a Code Assist SSE stream; the host enforces the
/// real transfer limits, this only keeps the decoder's buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 8 * 1024 * 1024,
    max_value_bytes: 8 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 8 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

struct Codec {
    decoder: SseDecoder,
    encoder: SseEncoder,
}

impl Codec {
    fn new() -> Self {
        Self {
            decoder: SseDecoder::new(SSE_LIMITS),
            encoder: SseEncoder::new(SSE_LIMITS),
        }
    }

    fn emit(&mut self, frames: Vec<SseFrame>) -> Result<Vec<Bytes>, ChannelError> {
        let mut output = Vec::with_capacity(frames.len());
        for frame in frames {
            let bytes = match frame {
                SseFrame::Done => self.encoder.done(),
                SseFrame::Event(event) => {
                    let value: Value = serde_json::from_str(&event.data).map_err(|error| {
                        invalid_response(format!("Code Assist SSE payload JSON: {error}"))
                    })?;
                    let mut inner = unwrap_value(&value).clone();
                    normalize_content(&mut inner);
                    let data = serde_json::to_string(&inner)
                        .map_err(|error| invalid_response(error.to_string()))?;
                    self.encoder.event(&SseEvent { data, ..event })
                }
            }
            .map_err(|error| invalid_response(error.to_string()))?;
            output.push(bytes);
        }
        Ok(output)
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self
            .decoder
            .push(chunk)
            .map_err(|error| invalid_response(error.to_string()))?;
        self.emit(frames)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self
            .decoder
            .finish()
            .map_err(|error| invalid_response(error.to_string()))?;
        self.emit(frames)
    }
}

/// Re-emit an SSE body with every payload unwrapped. A buffered body is
/// treated as a single chunk, so a stream the host already collected keeps
/// working.
pub(crate) fn unwrap_sse(body: HttpBody) -> HttpBody {
    let upstream: ByteStream = match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(stream::once(async move { Ok(bytes) })),
    };
    let state = (
        upstream,
        Codec::new(),
        VecDeque::<Result<Bytes, TransportError>>::new(),
        false,
    );
    HttpBody::Stream(Box::pin(stream::unfold(
        state,
        |(mut upstream, mut codec, mut queue, mut ended)| async move {
            loop {
                if let Some(next) = queue.pop_front() {
                    return Some((next, (upstream, codec, queue, ended)));
                }
                if ended {
                    return None;
                }
                let result = match upstream.next().await {
                    Some(Ok(bytes)) => codec.push(&bytes),
                    Some(Err(error)) => {
                        ended = true;
                        queue.push_back(Err(error));
                        continue;
                    }
                    None => {
                        ended = true;
                        codec.finish()
                    }
                };
                match result {
                    Ok(bytes) => queue.extend(bytes.into_iter().map(Ok)),
                    Err(error) => {
                        ended = true;
                        queue.push_back(Err(Box::new(error)));
                    }
                }
            }
        },
    )))
}
