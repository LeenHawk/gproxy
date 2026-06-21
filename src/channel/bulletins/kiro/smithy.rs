//! AWS Smithy event-stream binary frame parser (`vnd.amazon.eventstream`).
//!
//! Kiro's `POST /generateAssistantResponse` responds with an AWS event-stream:
//! a sequence of length-prefixed binary frames. Each frame is:
//!
//! ```text
//!   prelude (12B):  total_len:u32-BE  headers_len:u32-BE  prelude_crc:u32-BE
//!   headers:        headers_len bytes of TLV (see [`parse_headers`])
//!   payload:        total_len - headers_len - 16 bytes (JSON for Kiro events)
//!   message_crc:    u32-BE
//! ```
//!
//! [`SmithyFrameParser`] accepts bytes incrementally ([`push`](SmithyFrameParser::push)
//! per upstream chunk) and drains every COMPLETE frame, buffering a partial tail
//! until the rest of its bytes arrive — so a frame split across network chunks
//! is still parsed exactly once. Ported verbatim from the v1 `gproxy-channel`
//! kiro impl; like v1 it does NOT validate the CRCs (they are read past). The
//! parser is fully synchronous, so it compiles on the wasm edge target.

use serde_json::Value;

/// A decoded event-stream frame: the `:event-type` header (when present) plus
/// the JSON payload (`Value::Null` when the frame has an empty payload).
#[derive(Debug)]
pub struct SmithyFrame {
    pub event_type: Option<String>,
    pub payload: Value,
}

/// Incremental event-stream frame parser. Buffers raw upstream bytes and yields
/// frames as soon as each is complete; a partial trailing frame is retained for
/// the next [`push`](SmithyFrameParser::push).
#[derive(Debug, Default)]
pub struct SmithyFrameParser {
    pending: Vec<u8>,
}

impl SmithyFrameParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw chunk; return every frame that is now complete. Frames whose
    /// bytes have not fully arrived stay buffered. A malformed frame length
    /// terminates parsing for this call (the bad bytes stay buffered, so no
    /// progress past corruption — matching v1's fail-closed behaviour).
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SmithyFrame> {
        self.pending.extend_from_slice(chunk);
        let mut frames = Vec::new();
        let mut offset = 0usize;
        // Need at least the 12-byte prelude to read total_len.
        while self.pending.len().saturating_sub(offset) >= 12 {
            let total_len = be_u32(&self.pending[offset..]) as usize;
            // A valid frame is prelude(12) + crc(4) at minimum.
            if total_len < 16 {
                break;
            }
            if self.pending.len().saturating_sub(offset) < total_len {
                break; // frame not fully arrived yet — wait for more bytes.
            }
            if let Some(frame) = decode_frame(&self.pending[offset..offset + total_len]) {
                frames.push(frame);
            }
            offset += total_len;
        }
        if offset > 0 {
            self.pending.drain(..offset);
        }
        frames
    }
}

/// Read a big-endian u32 from the front of `bytes` (caller guarantees ≥4 bytes).
fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Decode a single COMPLETE frame (caller guarantees `frame.len() == total_len`,
/// `total_len >= 16`). Returns `None` if the header span is malformed or the
/// payload is non-empty but not valid JSON.
fn decode_frame(frame: &[u8]) -> Option<SmithyFrame> {
    let total_len = be_u32(&frame[0..]) as usize;
    let headers_len = be_u32(&frame[4..]) as usize;
    let headers_start = 12usize;
    let headers_end = headers_start.checked_add(headers_len)?;
    let payload_end = total_len.checked_sub(4)?; // trailing message_crc:u32
    if headers_end > payload_end || payload_end > frame.len() {
        return None;
    }
    let headers = parse_headers(&frame[headers_start..headers_end]);
    let payload = if headers_end == payload_end {
        Value::Null
    } else {
        serde_json::from_slice(&frame[headers_end..payload_end]).ok()?
    };
    Some(SmithyFrame {
        event_type: headers.get(":event-type").cloned(),
        payload,
    })
}

/// Parse the event-stream header TLV block into a name→string map.
///
/// Each header is `name_len:u8, name:utf8, value_type:u8, value...`. The ten AWS
/// value types are handled; non-string scalars are stringified (so numeric
/// headers remain readable), bool/timestamp likewise, and uuid/bytes-array are
/// skipped (their bytes are still consumed to stay frame-aligned). A truncated
/// header aborts parsing and returns whatever was decoded so far — `decode_frame`
/// only needs `:event-type`, which precedes any binary headers in practice.
fn parse_headers(mut bytes: &[u8]) -> std::collections::BTreeMap<String, String> {
    let mut headers = std::collections::BTreeMap::new();
    while !bytes.is_empty() {
        let name_len = bytes[0] as usize;
        bytes = &bytes[1..];
        if bytes.len() < name_len + 1 {
            break;
        }
        let Ok(name) = std::str::from_utf8(&bytes[..name_len]) else {
            break;
        };
        let name = name.to_string();
        bytes = &bytes[name_len..];
        let value_type = bytes[0];
        bytes = &bytes[1..];
        match value_type {
            // 0 = true, 1 = false (no value bytes).
            0 => {
                headers.insert(name, "true".to_string());
            }
            1 => {
                headers.insert(name, "false".to_string());
            }
            // 2 = byte (i8).
            2 => {
                if bytes.is_empty() {
                    break;
                }
                headers.insert(name, (bytes[0] as i8).to_string());
                bytes = &bytes[1..];
            }
            // 3 = short (i16-BE).
            3 => {
                if bytes.len() < 2 {
                    break;
                }
                headers.insert(name, i16::from_be_bytes([bytes[0], bytes[1]]).to_string());
                bytes = &bytes[2..];
            }
            // 4 = integer (i32-BE).
            4 => {
                if bytes.len() < 4 {
                    break;
                }
                headers.insert(
                    name,
                    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string(),
                );
                bytes = &bytes[4..];
            }
            // 5 = long (i64-BE), 8 = timestamp (epoch millis, i64-BE).
            5 | 8 => {
                if bytes.len() < 8 {
                    break;
                }
                headers.insert(
                    name,
                    i64::from_be_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ])
                    .to_string(),
                );
                bytes = &bytes[8..];
            }
            // 6 = byte array (len-prefixed, skipped), 7 = string (len-prefixed).
            6 | 7 => {
                if bytes.len() < 2 {
                    break;
                }
                let len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
                bytes = &bytes[2..];
                if bytes.len() < len {
                    break;
                }
                if value_type == 7
                    && let Ok(value) = std::str::from_utf8(&bytes[..len])
                {
                    headers.insert(name, value.to_string());
                }
                bytes = &bytes[len..];
            }
            // 9 = uuid (16 bytes, skipped).
            9 => {
                if bytes.len() < 16 {
                    break;
                }
                bytes = &bytes[16..];
            }
            _ => break,
        }
    }
    headers
}

/// Hand-build a valid event-stream frame with a single `:event-type` string
/// header and a JSON payload. Mirrors the wire format Kiro emits — shared by the
/// [`smithy`](self) + [`response`](super::response) test modules.
#[cfg(test)]
pub(super) fn build_frame(event_type: &str, payload: &[u8]) -> Vec<u8> {
    // header: name_len:u8 | name | type(7) | value_len:u16-BE | value
    let name = b":event-type";
    let mut headers = Vec::new();
    headers.push(name.len() as u8);
    headers.extend_from_slice(name);
    headers.push(7u8); // string
    headers.extend_from_slice(&(event_type.len() as u16).to_be_bytes());
    headers.extend_from_slice(event_type.as_bytes());

    let headers_len = headers.len() as u32;
    let total_len = (12 + headers.len() + payload.len() + 4) as u32;
    let mut frame = Vec::with_capacity(total_len as usize);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&headers_len.to_be_bytes());
    frame.extend_from_slice(&0u32.to_be_bytes()); // prelude_crc (unchecked)
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&0u32.to_be_bytes()); // message_crc (unchecked)
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smithy_frame_parse() {
        let frame = build_frame("assistantResponseEvent", br#"{"content":"hi"}"#);

        // Whole frame in one push.
        let mut parser = SmithyFrameParser::new();
        let frames = parser.push(&frame);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].event_type.as_deref(),
            Some("assistantResponseEvent")
        );
        assert_eq!(frames[0].payload["content"], "hi");

        // CHUNK BOUNDARY: split the frame in two halves → still yields once, only
        // after the second half arrives.
        let mut parser = SmithyFrameParser::new();
        let mid = frame.len() / 2;
        let first = parser.push(&frame[..mid]);
        assert!(first.is_empty(), "partial frame must not yield yet");
        let second = parser.push(&frame[mid..]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].payload["content"], "hi");
    }
}
