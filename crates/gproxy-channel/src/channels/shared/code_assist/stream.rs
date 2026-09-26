//! Unwrapping the Code Assist envelope out of a delivered response stream.
//!
//! The upstream answers `:streamGenerateContent?alt=sse` with SSE whose every
//! `data:` payload is `{"response": {...}}`; the channel declares Gemini as
//! its native dialect, so each payload is re-emitted as the bare Gemini
//! chunk before the host or the client ever sees it (v3
//! `shared/code_assist/stream.rs`, which did the same inside a
//! `StreamDecoder`). Framing and event ordering are preserved; only the JSON
//! nesting changes.

use super::usage::SSE_LIMITS;
use super::{invalid_response, normalize_content, unwrap_value};
use crate::channel::ChannelError;
use futures_util::{StreamExt, stream};
use gproxy_protocol::HttpBody;
use gproxy_protocol::codec::{SseDecoder, SseEncoder, SseEvent, SseFrame};
use gproxy_protocol::connection::{ByteStream, Bytes, TransportError};
use serde_json::{Map, Value};
use std::collections::VecDeque;

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

/// Fold a collected `alt=sse` reply into the one buffered Gemini response
/// `:generateContent` would have returned, for a model whose buffered call
/// loses what the stream carries. Parts are appended in order with runs of
/// plain text (or plain thought) joined, a thought signature the stream sent
/// on its own empty part joins the thought run before it, and the last
/// finish reason, usage and response metadata win.
pub(crate) fn aggregate(body: &[u8]) -> Result<Value, ChannelError> {
    let mut decoder = SseDecoder::new(SSE_LIMITS);
    let mut frames = decoder
        .push(body)
        .map_err(|error| invalid_response(error.to_string()))?;
    frames.extend(
        decoder
            .finish()
            .map_err(|error| invalid_response(error.to_string()))?,
    );
    let mut parts: Vec<Value> = Vec::new();
    let mut candidate = Map::new();
    let mut response = Map::new();
    for frame in frames {
        let SseFrame::Event(event) = frame else {
            continue;
        };
        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| invalid_response(format!("Code Assist SSE payload JSON: {error}")))?;
        let Value::Object(chunk) = unwrap_value(&value).clone() else {
            continue;
        };
        for (key, value) in chunk {
            if key != "candidates" {
                response.insert(key, value);
                continue;
            }
            let Some(first) = value.as_array().and_then(|list| list.first()) else {
                continue;
            };
            for (key, value) in first.as_object().into_iter().flatten() {
                if key == "content" {
                    for part in value
                        .get("parts")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        append(&mut parts, part.clone());
                    }
                } else {
                    candidate.insert(key.clone(), value.clone());
                }
            }
        }
    }
    parts.retain(|part| !(plain(part) && part.get("text").and_then(Value::as_str) == Some("")));
    candidate.insert(
        "content".into(),
        serde_json::json!({ "role": "model", "parts": parts }),
    );
    candidate.entry("index").or_insert(Value::from(0));
    response.insert(
        "candidates".into(),
        Value::Array(vec![Value::Object(candidate)]),
    );
    let mut response = Value::Object(response);
    normalize_content(&mut response);
    Ok(response)
}

fn thought(part: &Value) -> bool {
    part.get("thought").and_then(Value::as_bool) == Some(true)
}

/// Text (thought or not) with nothing else beside it.
fn plain(part: &Value) -> bool {
    part.as_object().is_some_and(|object| {
        object.get("text").is_some_and(Value::is_string)
            && object
                .keys()
                .all(|key| matches!(key.as_str(), "text" | "thought"))
    })
}

fn append(parts: &mut Vec<Value>, part: Value) {
    let Some(last) = parts.last_mut() else {
        parts.push(part);
        return;
    };
    let signature_only = thought(&part)
        && part
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        && part.get("thoughtSignature").is_some();
    if thought(last) && plain(last) && signature_only {
        last["thoughtSignature"] = part["thoughtSignature"].clone();
        return;
    }
    if plain(last) && plain(&part) && thought(last) == thought(&part) {
        let text = part["text"].as_str().unwrap_or_default().to_owned();
        if let Some(Value::String(existing)) = last.get_mut("text") {
            existing.push_str(&text);
        }
        return;
    }
    parts.push(part);
}
