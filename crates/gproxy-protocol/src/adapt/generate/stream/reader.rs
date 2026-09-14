//! Transport framing only. Native event validation belongs to the concrete pair.
use crate::{
    HttpBody,
    codec::{self, CodecError, CodecErrorKind, CodecErrorStage, CodecLimits},
    connection::ByteStream,
    wire::DeclaredFields,
};
use bytes::{Buf, Bytes};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFraming {
    Sse,
    JsonArray,
    Ndjson,
}
#[derive(Debug)]
pub enum NativeFrame<T> {
    Event { name: Option<String>, value: T },
    Done,
}
enum Decoder {
    Sse(codec::SseDecoder),
    Array(codec::JsonArrayDecoder),
    Ndjson(codec::NdjsonDecoder),
}
enum Frame {
    Sse(codec::SseFrame),
    Json(serde_json::Value),
}
/// Reads only until another framed event is available. Dropping `next` while
/// awaiting another body chunk retains decoder state and the current chunk.
/// A decoder/transport error poisons this reader and releases its transport;
/// transport is never replayed. Large/empty ready input cooperatively yields
/// so cancellation and host deadlines remain observable.
pub struct NativeReader {
    stream: Option<ByteStream>,
    pending: Bytes,
    decoder: Decoder,
    queued: VecDeque<Frame>,
    limits: CodecLimits,
    received: u64,
    events: usize,
    max_events: usize,
    eof: bool,
    failed: bool,
}
impl NativeReader {
    pub fn new(
        body: HttpBody,
        framing: SourceFraming,
        limits: CodecLimits,
        max_events: usize,
    ) -> Self {
        let (stream, pending) = match body {
            HttpBody::Bytes(bytes) => (None, bytes),
            HttpBody::Stream(stream) => (Some(stream), Bytes::new()),
        };
        Self {
            received: pending.len() as u64,
            stream,
            pending,
            decoder: match framing {
                SourceFraming::Sse => Decoder::Sse(codec::SseDecoder::new(limits)),
                SourceFraming::JsonArray => Decoder::Array(codec::JsonArrayDecoder::new(limits)),
                SourceFraming::Ndjson => Decoder::Ndjson(codec::NdjsonDecoder::new(limits)),
            },
            queued: VecDeque::new(),
            limits,
            events: 0,
            max_events,
            eof: false,
            failed: false,
        }
    }
    pub async fn next<T: DeserializeOwned + DeclaredFields>(
        &mut self,
    ) -> Result<Option<NativeFrame<T>>, CodecError> {
        if self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                "native reader terminated after failure",
            ));
        }
        let result = self.next_inner().await;
        if result.is_err() {
            self.failed = true;
            self.stream = None;
            self.pending = Bytes::new();
            self.queued.clear();
        }
        result
    }
    async fn next_inner<T: DeserializeOwned + DeclaredFields>(
        &mut self,
    ) -> Result<Option<NativeFrame<T>>, CodecError> {
        if self.received > self.limits.max_body_bytes {
            return Err(error(
                CodecErrorKind::Limit,
                "native body byte limit exceeded",
            ));
        }
        let mut work = 0usize;
        loop {
            if let Some(frame) = self.queued.pop_front() {
                if self.events >= self.max_events {
                    return Err(error(CodecErrorKind::Limit, "native event limit exceeded"));
                }
                self.events += 1;
                return match frame {
                    Frame::Sse(codec::SseFrame::Done) => Ok(Some(NativeFrame::Done)),
                    Frame::Sse(codec::SseFrame::Event(event)) => {
                        let value: T = codec::decode_json(event.data.as_bytes(), self.limits)?;
                        Ok(Some(NativeFrame::Event {
                            name: event.event,
                            value: value.into_declared(),
                        }))
                    }
                    Frame::Json(value) => {
                        let value: T = serde_json::from_value(value).map_err(|e| {
                            CodecError::with_source(
                                CodecErrorKind::Json,
                                CodecErrorStage::Stream,
                                "invalid native event",
                                e,
                            )
                        })?;
                        Ok(Some(NativeFrame::Event {
                            name: None,
                            value: value.into_declared(),
                        }))
                    }
                };
            }
            if work >= 1024 {
                yield_once().await;
                work = 0;
            }
            if !self.pending.is_empty() {
                // Stop exactly when a value becomes available. Feeding a larger
                // array batch could let a later syntax error swallow a valid
                // prefix event from the same transport chunk. SSE/NDJSON need
                // at most one physical line to dispatch the next frame.
                let count = match &self.decoder {
                    Decoder::Array(_) => 1,
                    _ => self
                        .pending
                        .iter()
                        .position(|b| *b == b'\n' || *b == b'\r')
                        .map_or(self.pending.len(), |n| n + 1),
                };
                match &mut self.decoder {
                    Decoder::Sse(d) => self
                        .queued
                        .extend(d.push(&self.pending[..count])?.into_iter().map(Frame::Sse)),
                    Decoder::Array(d) => self
                        .queued
                        .extend(d.push(&self.pending[..count])?.into_iter().map(Frame::Json)),
                    Decoder::Ndjson(d) => self
                        .queued
                        .extend(d.push(&self.pending[..count])?.into_iter().map(Frame::Json)),
                }
                self.pending.advance(count);
                work += 1;
                continue;
            }
            if self.eof {
                return Ok(None);
            }
            if let Some(stream) = &mut self.stream {
                if let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| {
                        CodecError::with_source(
                            CodecErrorKind::Transport,
                            CodecErrorStage::Stream,
                            "native stream transport failed",
                            e,
                        )
                    })?;
                    self.received = self
                        .received
                        .checked_add(chunk.len() as u64)
                        .ok_or_else(|| error(CodecErrorKind::Limit, "native body size overflow"))?;
                    if self.received > self.limits.max_body_bytes {
                        return Err(error(
                            CodecErrorKind::Limit,
                            "native body byte limit exceeded",
                        ));
                    }
                    self.pending = chunk;
                    work += 1;
                    continue;
                }
                self.stream = None;
            }
            self.eof = true;
            match &mut self.decoder {
                Decoder::Sse(d) => self.queued.extend(d.finish()?.into_iter().map(Frame::Sse)),
                Decoder::Array(d) => d.finish()?,
                Decoder::Ndjson(d) => self.queued.extend(d.finish()?.into_iter().map(Frame::Json)),
            }
        }
    }
}
fn error(kind: CodecErrorKind, message: &'static str) -> CodecError {
    CodecError::new(kind, CodecErrorStage::Stream, message)
}

async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}
