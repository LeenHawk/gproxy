//! AWS `vnd.amazon.eventstream` framing (v3 `shared/aws_eventstream/`).
//!
//! Several AWS-shaped upstreams do not answer a streaming call with SSE. The
//! body is a sequence of self-describing frames:
//!
//! ```text
//! u32 total length | u32 headers length | u32 prelude CRC32
//! headers (u8 name length, name, u8 value type, value)
//! payload
//! u32 message CRC32
//! ```
//!
//! Only the four `:`-prefixed headers those upstreams use are kept; every
//! other header is skipped by its declared value type. Nothing here knows what
//! a payload means — that is the asking channel's business.

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;

const PRELUDE_LEN: usize = 12;
const MIN_FRAME_LEN: usize = PRELUDE_LEN + 4;
/// AWS's documented event-stream maximum message size.
const MAX_FRAME_LEN: usize = 100 * 1024 * 1024;

/// One decoded event-stream message.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    pub(crate) message_type: Option<String>,
    pub(crate) event_type: Option<String>,
    pub(crate) exception_type: Option<String>,
    pub(crate) payload: Bytes,
}

impl Frame {
    /// The name this frame carries: an exception type wins over an event type,
    /// because an exception frame also names the event it replaced.
    pub(crate) fn name(&self) -> &str {
        self.exception_type
            .as_deref()
            .or(self.event_type.as_deref())
            .unwrap_or_default()
    }
}

/// Reassembles frames across arbitrary transport chunk boundaries.
#[derive(Debug, Default)]
pub(crate) struct FrameParser {
    pending: Vec<u8>,
}

impl FrameParser {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ChannelError> {
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
    pub(crate) fn finish(&self) -> Result<(), ChannelError> {
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

/// Encode one frame with a single `:event-type` string header. Only tests and
/// fixtures build frames; an upstream only ever sends them.
#[cfg(test)]
pub(crate) fn encode(event_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut headers = Vec::new();
    let name = b":event-type";
    headers.push(name.len() as u8);
    headers.extend_from_slice(name);
    headers.push(7);
    headers.extend_from_slice(&(event_type.len() as u16).to_be_bytes());
    headers.extend_from_slice(event_type.as_bytes());
    let total = (PRELUDE_LEN + headers.len() + payload.len() + 4) as u32;
    let mut frame = Vec::with_capacity(total as usize);
    frame.extend_from_slice(&total.to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32fast::hash(&frame[..8]).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    let crc = crc32fast::hash(&frame);
    frame.extend_from_slice(&crc.to_be_bytes());
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_reassemble_across_chunk_boundaries() {
        let mut bytes = encode("assistantResponseEvent", br#"{"content":"hi"}"#);
        bytes.extend(encode("metadataEvent", br#"{"tokenUsage":{}}"#));
        let mut parser = FrameParser::new();
        let mut frames = Vec::new();
        for chunk in bytes.chunks(7) {
            frames.extend(parser.push(chunk).unwrap());
        }
        parser.finish().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name(), "assistantResponseEvent");
        assert_eq!(&frames[0].payload[..], br#"{"content":"hi"}"#);
        assert_eq!(frames[1].name(), "metadataEvent");
    }

    #[test]
    fn a_truncated_tail_is_an_error_rather_than_a_clean_end() {
        let bytes = encode("assistantResponseEvent", br#"{"content":"hi"}"#);
        let mut parser = FrameParser::new();
        assert!(parser.push(&bytes[..bytes.len() - 3]).unwrap().is_empty());
        assert!(parser.finish().is_err());
    }

    #[test]
    fn a_corrupt_prelude_is_refused() {
        let mut bytes = encode("assistantResponseEvent", b"{}");
        bytes[4] = 0xff;
        assert!(FrameParser::new().push(&bytes).is_err());
    }
}
