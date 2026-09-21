//! Incremental backend SSE normalization with request-local tool aliases.
mod event;
mod lifecycle;
#[cfg(test)]
mod tests;
mod tools;

use super::usage::SSE_LIMITS;
use crate::{channel::ChannelError, channels::codex::shape::tools::Aliases};
use event::{Event, invalid};
use futures_util::{StreamExt, stream};
use gproxy_protocol::{
    HttpBody,
    codec::{SseDecoder, SseEncoder, SseEvent, SseFrame},
    connection::{ByteStream, Bytes, TransportError},
};
use std::collections::VecDeque;

struct Codec {
    decoder: SseDecoder,
    encoder: SseEncoder,
    lifecycle: lifecycle::Lifecycle,
    started: bool,
    sequence: i64,
    pending: Vec<(Event, SseEvent)>,
    pending_bytes: usize,
}

impl Codec {
    fn new(aliases: Aliases) -> Self {
        Self {
            decoder: SseDecoder::new(SSE_LIMITS),
            encoder: SseEncoder::new(SSE_LIMITS),
            lifecycle: lifecycle::Lifecycle::new(aliases),
            started: false,
            sequence: 0,
            pending: Vec::new(),
            pending_bytes: 0,
        }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.push(bytes).map_err(invalid)?;
        self.frames(frames)
    }
    fn frames(&mut self, frames: Vec<SseFrame>) -> Result<Vec<Bytes>, ChannelError> {
        let mut output = Vec::new();
        for frame in frames {
            let source = match frame {
                SseFrame::Done => {
                    if !self.lifecycle.terminal {
                        return Err(invalid("[DONE] before terminal response"));
                    }
                    output.push(self.encoder.done().map_err(invalid)?);
                    continue;
                }
                SseFrame::Event(source) => source,
            };
            let event: Event = serde_json::from_str(&source.data).map_err(invalid)?;
            if !self.started {
                if event.kind == "response.created" {
                    self.started = true;
                    output.extend(self.normalize(event, &source)?);
                    for (pending, original) in std::mem::take(&mut self.pending) {
                        output.extend(self.normalize(pending, &original)?);
                    }
                    self.pending_bytes = 0;
                    continue;
                }
                if event.kind == "error" {
                    self.pending.clear();
                    self.pending_bytes = 0;
                    output.extend(self.normalize(event, &source)?);
                    continue;
                }
                if let Some(response) = event.response.as_ref().filter(|response| {
                    response
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some()
                }) {
                    self.started = true;
                    output.extend(self.normalize(event::created(response)?, &source)?);
                    for (pending, original) in std::mem::take(&mut self.pending) {
                        output.extend(self.normalize(pending, &original)?);
                    }
                    self.pending_bytes = 0;
                } else {
                    if event.terminal() {
                        return Err(invalid(
                            "cannot repair response.created without response id",
                        ));
                    }
                    // Wait for real response metadata rather than inventing an id
                    // that cannot be used for previous_response_id continuations.
                    self.pending_bytes += source.data.len();
                    if self.pending_bytes as u64 > SSE_LIMITS.max_buffer_bytes {
                        return Err(invalid("missing response metadata exceeds buffer limit"));
                    }
                    self.pending.push((event, source));
                    continue;
                }
            }
            output.extend(self.normalize(event, &source)?);
        }
        Ok(output)
    }
    fn normalize(&mut self, event: Event, source: &SseEvent) -> Result<Vec<Bytes>, ChannelError> {
        self.lifecycle
            .normalize(event)?
            .into_iter()
            .map(|mut event| {
                // Inserted and suppressed events need a fresh monotonic sequence.
                event.sequence_number = Some(self.sequence);
                self.sequence += 1;
                self.encoder
                    .event(&SseEvent {
                        event: Some(event.kind.clone()),
                        id: source.id.clone(),
                        retry: source.retry,
                        data: serde_json::to_string(&event).map_err(invalid)?,
                    })
                    .map_err(invalid)
            })
            .collect()
    }
    fn finish(&mut self) -> Result<Vec<Bytes>, ChannelError> {
        let frames = self.decoder.finish().map_err(invalid)?;
        let output = self.frames(frames)?;
        if !self.lifecycle.terminal {
            return Err(invalid("stream ended without a terminal response event"));
        }
        Ok(output)
    }
}

pub(super) fn normalize(body: HttpBody, aliases: Aliases) -> HttpBody {
    let upstream: ByteStream = match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(stream::once(async move { Ok(bytes) })),
    };
    let state = (
        upstream,
        Codec::new(aliases),
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
