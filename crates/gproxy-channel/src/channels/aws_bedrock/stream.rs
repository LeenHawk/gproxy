//! AWS event-stream framing, and its translation into Claude Messages SSE.
//!
//! `InvokeModelWithResponseStream` does not answer with SSE. Its body is a
//! sequence of `vnd.amazon.eventstream` frames (v3
//! `shared/aws_eventstream/` and `aws_bedrock/sse/invoke.rs`), each one:
//!
//! ```text
//! u32 total length | u32 headers length | u32 prelude CRC32
//! headers (u8 name length, name, u8 value type, value)
//! payload
//! u32 message CRC32
//! ```
//!
//! For an Anthropic model every `:event-type: chunk` frame carries a JSON
//! payload whose `bytes` field is base64 of one Claude Messages SSE event
//! object. Re-emitting those as `event: {type}\ndata: {json}\n\n` gives the
//! client exactly the Messages stream it asked for, so this is the only place
//! the channel rewrites a delivered body. A frame whose `:message-type` is
//! `exception` becomes a Messages `error` event and closes the stream.

use crate::channel::ChannelError;
use base64::Engine as _;
use gproxy_protocol::connection::{Bytes, TransportError};
use serde_json::{Value, json};

const PRELUDE_LEN: usize = 12;
const MIN_FRAME_LEN: usize = PRELUDE_LEN + 4;
/// AWS's documented event-stream maximum message size.
const MAX_FRAME_LEN: usize = 100 * 1024 * 1024;

/// One decoded event-stream message. Only the four `:`-prefixed headers
/// Bedrock uses are kept; the rest are skipped by type.
#[derive(Debug, Default)]
pub(super) struct Frame {
    pub(super) message_type: Option<String>,
    pub(super) event_type: Option<String>,
    pub(super) exception_type: Option<String>,
    pub(super) payload: Bytes,
}

/// Reassembles frames across arbitrary transport chunk boundaries.
#[derive(Debug, Default)]
pub(super) struct FrameParser {
    pending: Vec<u8>,
}

impl FrameParser {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ChannelError> {
        self.pending.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            if self.pending.len() < PRELUDE_LEN {
                return Ok(frames);
            }
            let layout = decode_prelude(&self.pending[..PRELUDE_LEN])?;
            if self.pending.len() < layout.total_len {
                return Ok(frames);
            }
            let raw = self.pending.drain(..layout.total_len).collect::<Vec<_>>();
            frames.push(decode_frame(&raw, layout)?);
        }
    }

    /// A clean EOF leaves nothing half-read.
    pub(super) fn finish(&self) -> Result<(), ChannelError> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(decode(format!(
                "the stream ended inside a frame after {} bytes",
                self.pending.len()
            )))
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    total_len: usize,
    headers_len: usize,
}

fn decode_prelude(prelude: &[u8]) -> Result<Layout, ChannelError> {
    let total_len = read_u32(&prelude[..4]) as usize;
    if !(MIN_FRAME_LEN..=MAX_FRAME_LEN).contains(&total_len) {
        return Err(decode(format!(
            "frame length {total_len} is outside {MIN_FRAME_LEN}..={MAX_FRAME_LEN}"
        )));
    }
    let headers_len = read_u32(&prelude[4..8]) as usize;
    if headers_len > total_len - MIN_FRAME_LEN {
        return Err(decode(format!(
            "headers length {headers_len} exceeds frame length {total_len}"
        )));
    }
    if crc32fast::hash(&prelude[..8]) != read_u32(&prelude[8..12]) {
        return Err(decode("prelude CRC mismatch"));
    }
    Ok(Layout {
        total_len,
        headers_len,
    })
}

fn decode_frame(raw: &[u8], layout: Layout) -> Result<Frame, ChannelError> {
    let message_end = layout.total_len - 4;
    if crc32fast::hash(&raw[..message_end]) != read_u32(&raw[message_end..]) {
        return Err(decode("message CRC mismatch"));
    }
    let headers_end = PRELUDE_LEN + layout.headers_len;
    let mut frame = decode_headers(&raw[PRELUDE_LEN..headers_end])?;
    frame.payload = Bytes::copy_from_slice(&raw[headers_end..message_end]);
    Ok(frame)
}

fn decode_headers(mut bytes: &[u8]) -> Result<Frame, ChannelError> {
    let mut frame = Frame::default();
    while !bytes.is_empty() {
        let name_len = usize::from(take(&mut bytes, 1)?[0]);
        let name = std::str::from_utf8(take(&mut bytes, name_len)?)
            .map_err(|_| decode("header name is not UTF-8"))?
            .to_owned();
        let kind = take(&mut bytes, 1)?[0];
        let Some(value) = header_value(kind, &mut bytes)? else {
            continue;
        };
        match name.as_str() {
            ":message-type" => frame.message_type = Some(value),
            ":event-type" => frame.event_type = Some(value),
            ":exception-type" => frame.exception_type = Some(value),
            _ => {}
        }
    }
    Ok(frame)
}

/// Only the string type (7) yields a value; the rest are skipped by their
/// fixed or length-prefixed width.
fn header_value(kind: u8, bytes: &mut &[u8]) -> Result<Option<String>, ChannelError> {
    let skip = match kind {
        0 | 1 => 0,
        2 => 1,
        3 => 2,
        4 => 4,
        5 | 8 => 8,
        6 | 7 => {
            let length = take(bytes, 2)?;
            let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
            let value = take(bytes, length)?;
            return if kind == 6 {
                Ok(None)
            } else {
                std::str::from_utf8(value)
                    .map(|value| Some(value.to_owned()))
                    .map_err(|_| decode("string header value is not UTF-8"))
            };
        }
        9 => 16,
        other => return Err(decode(format!("unknown header value type {other}"))),
    };
    take(bytes, skip)?;
    Ok(None)
}

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], ChannelError> {
    if bytes.len() < length {
        return Err(decode("the header block is truncated"));
    }
    let (head, tail) = bytes.split_at(length);
    *bytes = tail;
    Ok(head)
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("AWS event-stream: {}", message.into()))
}

/// Event-stream frames in, Claude Messages SSE bytes out.
#[derive(Default)]
pub(super) struct Translator {
    parser: FrameParser,
    stopped: bool,
    failed: bool,
}

impl Translator {
    pub(super) fn new() -> Self {
        Self {
            parser: FrameParser::new(),
            stopped: false,
            failed: false,
        }
    }

    /// True once an exception frame ended the message; the caller stops
    /// reading the upstream body.
    pub(super) fn failed(&self) -> bool {
        self.failed
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Bytes, ChannelError> {
        let mut out = Vec::new();
        for frame in self.parser.push(chunk)? {
            if frame.exception_type.is_some() || frame.message_type.as_deref() == Some("exception")
            {
                self.failed = true;
                let kind = frame
                    .exception_type
                    .as_deref()
                    .unwrap_or("stream_exception")
                    .to_owned();
                let message = serde_json::from_slice::<Value>(&frame.payload)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| kind.clone());
                out.extend_from_slice(&encode(&json!({
                    "type": "error",
                    "error": {"type": kind, "message": message}
                })));
                break;
            }
            if frame.event_type.as_deref() != Some("chunk") {
                continue;
            }
            let encoded = serde_json::from_slice::<Value>(&frame.payload)
                .ok()
                .as_ref()
                .and_then(|payload| payload.get("bytes"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| decode("a chunk frame carries no base64 `bytes`"))?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| decode(format!("chunk base64: {error}")))?;
            let event: Value = serde_json::from_slice(&decoded)
                .map_err(|error| decode(format!("chunk payload JSON: {error}")))?;
            self.stopped |= event.get("type").and_then(Value::as_str) == Some("message_stop");
            out.extend_from_slice(&encode(&event));
        }
        Ok(Bytes::from(out))
    }

    /// Upstream EOF. A clean end must have delivered `message_stop`.
    pub(super) fn finish(&mut self) -> Result<(), ChannelError> {
        if self.failed {
            return Ok(());
        }
        self.parser.finish()?;
        if self.stopped {
            Ok(())
        } else {
            Err(decode("the stream ended before message_stop"))
        }
    }
}

/// `event: {type}\ndata: {json}\n\n`, the Claude Messages SSE framing.
pub(super) fn encode(event: &Value) -> Bytes {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    Bytes::from(format!("event: {name}\ndata: {event}\n\n"))
}

pub(super) fn transport_error(error: ChannelError) -> TransportError {
    error.to_string().into()
}
