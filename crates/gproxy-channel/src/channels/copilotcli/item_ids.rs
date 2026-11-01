//! One id per output item on Copilot's Responses stream.
//!
//! Copilot's `/responses` gives an output item a new id in every event: the
//! `response.output_item.added`, each delta, the `.done` and the final
//! `response.completed` output each carry another (magpie
//! `internal/gateway/itemids.go`, a real account on 2026-10-19; LiteLLM's
//! `github_copilot/responses/transformation.py` `_normalize_stream_item_id`).
//! A client that keys an item by its id (Codex starts one by the added's id
//! and finishes it by the done's; the Vercel AI SDK looks parts up by id)
//! then shows a message twice or never finishes one. Each event of an item is
//! put back under the id its `output_item.added` gave it, keyed by
//! `output_index`; an event whose ids already agree passes through untouched,
//! and Copilot accepts that id paired with the item's content on the next
//! turn.

use crate::channel::ChannelError;
use futures_util::{StreamExt, stream};
use gproxy_protocol::HttpBody;
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseEncoder, SseEvent, SseFrame};
use gproxy_protocol::connection::{ByteStream, Bytes, TransportError};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

/// Bounds for re-framing the stream; the host enforces the real transfer
/// limits, this only keeps the decoder's buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 8 * 1024 * 1024,
    max_value_bytes: 8 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 8 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

fn invalid(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidResponse(error.to_string())
}

/// The id and type each `output_index` was added with.
#[derive(Default)]
struct Ids(HashMap<u64, (String, Option<String>)>);

impl Ids {
    /// `item` is the one added at `index`, or another item type there.
    fn same_item(&self, index: u64, item: &Value) -> Option<&str> {
        let (id, kind) = self.0.get(&index)?;
        let other = item.get("type").and_then(Value::as_str);
        match (kind.as_deref(), other) {
            (Some(kind), Some(other)) if kind != other => None,
            _ => Some(id),
        }
    }

    /// The payload with every item id put back, or `None` when it already
    /// agreed (or is not JSON this concerns).
    fn rewrite(&mut self, data: &str) -> Option<String> {
        let mut event: Value = serde_json::from_str(data).ok()?;
        let kind = event.get("type").and_then(Value::as_str)?.to_owned();
        match kind.as_str() {
            "response.output_item.added" => {
                let index = event.get("output_index").and_then(Value::as_u64)?;
                let item = event.get("item")?;
                let id = item.get("id").and_then(Value::as_str)?.to_owned();
                let item_type = item.get("type").and_then(Value::as_str).map(str::to_owned);
                self.0.insert(index, (id, item_type));
                None
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                let output = event
                    .pointer_mut("/response/output")
                    .and_then(Value::as_array_mut)?;
                let mut changed = false;
                for (index, item) in output.iter_mut().enumerate() {
                    let Some(id) = self.same_item(index as u64, item).map(str::to_owned) else {
                        continue;
                    };
                    if let Some(slot) = item.get_mut("id")
                        && slot.as_str() != Some(id.as_str())
                    {
                        *slot = Value::String(id);
                        changed = true;
                    }
                }
                changed.then(|| event.to_string())
            }
            _ => {
                let index = event.get("output_index").and_then(Value::as_u64)?;
                let (id, _) = self.0.get(&index)?;
                let id = id.clone();
                let mut changed = false;
                if let Some(slot) = event.get_mut("item_id")
                    && slot.as_str() != Some(id.as_str())
                {
                    *slot = Value::String(id.clone());
                    changed = true;
                }
                let same = event
                    .get("item")
                    .is_some_and(|item| self.same_item(index, item).is_some());
                if same
                    && let Some(slot) = event.pointer_mut("/item/id")
                    && slot.as_str() != Some(id.as_str())
                {
                    *slot = Value::String(id);
                    changed = true;
                }
                changed.then(|| event.to_string())
            }
        }
    }
}

struct Codec {
    decoder: SseDecoder,
    encoder: SseEncoder,
    ids: Ids,
}

impl Codec {
    fn emit(&mut self, frames: Vec<SseFrame>) -> Result<Vec<Bytes>, ChannelError> {
        frames
            .into_iter()
            .map(|frame| match frame {
                SseFrame::Done => self.encoder.done().map_err(invalid),
                SseFrame::Event(event) => {
                    let data = self.ids.rewrite(&event.data).unwrap_or(event.data);
                    self.encoder
                        .event(&SseEvent { data, ..event })
                        .map_err(invalid)
                }
            })
            .collect()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.push(chunk).map_err(invalid)?;
        self.emit(frames)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.finish().map_err(invalid)?;
        self.emit(frames)
    }
}

/// The Responses SSE body with each item under its first id.
pub(super) fn stabilize(body: HttpBody) -> HttpBody {
    let upstream: ByteStream = match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(stream::once(async move { Ok(bytes) })),
    };
    let codec = Codec {
        decoder: SseDecoder::new(SSE_LIMITS),
        encoder: SseEncoder::new(SSE_LIMITS),
        ids: Ids::default(),
    };
    let state = (
        upstream,
        codec,
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
