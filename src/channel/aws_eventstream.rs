//! Incremental parser for AWS Smithy `application/vnd.amazon.eventstream` frames.

use std::collections::BTreeMap;

use serde_json::Value;

const MIN_FRAME_LEN: usize = 16;
pub(crate) const MAX_FRAME_LEN: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct SmithyFrame {
    pub event_type: Option<String>,
    pub exception_type: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Default)]
pub(crate) struct SmithyFrameParser {
    pending: Vec<u8>,
}

impl SmithyFrameParser {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<SmithyFrame> {
        self.pending.extend_from_slice(chunk);
        let mut frames = Vec::new();
        let mut offset = 0;
        while self.pending.len().saturating_sub(offset) >= 12 {
            let total_len = be_u32(&self.pending[offset..]) as usize;
            if !(MIN_FRAME_LEN..=MAX_FRAME_LEN).contains(&total_len)
                || self.pending.len().saturating_sub(offset) < total_len
            {
                break;
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

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

pub(crate) fn looks_like_frame(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 4 {
        return None;
    }
    let total_len = be_u32(bytes) as usize;
    if !(MIN_FRAME_LEN..=MAX_FRAME_LEN).contains(&total_len) {
        return Some(false);
    }
    if bytes.len() < 12 {
        return None;
    }
    let headers_len = be_u32(&bytes[4..]) as usize;
    Some(headers_len <= total_len - MIN_FRAME_LEN)
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn decode_frame(frame: &[u8]) -> Option<SmithyFrame> {
    let total_len = be_u32(frame) as usize;
    let headers_len = be_u32(&frame[4..]) as usize;
    let headers_end = 12usize.checked_add(headers_len)?;
    let payload_end = total_len.checked_sub(4)?;
    if headers_end > payload_end || payload_end > frame.len() {
        return None;
    }
    let headers = parse_headers(&frame[12..headers_end]);
    let payload = if headers_end == payload_end {
        Value::Null
    } else {
        serde_json::from_slice(&frame[headers_end..payload_end]).ok()?
    };
    Some(SmithyFrame {
        event_type: headers.get(":event-type").cloned(),
        exception_type: headers.get(":exception-type").cloned(),
        payload,
    })
}

fn parse_headers(mut bytes: &[u8]) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    while !bytes.is_empty() {
        let name_len = bytes[0] as usize;
        bytes = &bytes[1..];
        if bytes.len() < name_len + 1 {
            break;
        }
        let Ok(name) = std::str::from_utf8(&bytes[..name_len]) else {
            break;
        };
        let name = name.to_owned();
        bytes = &bytes[name_len..];
        let value_type = bytes[0];
        bytes = &bytes[1..];
        let Some((value, consumed)) = parse_header_value(value_type, bytes) else {
            break;
        };
        if let Some(value) = value {
            headers.insert(name, value);
        }
        bytes = &bytes[consumed..];
    }
    headers
}

fn parse_header_value(kind: u8, bytes: &[u8]) -> Option<(Option<String>, usize)> {
    match kind {
        0 => Some((Some("true".into()), 0)),
        1 => Some((Some("false".into()), 0)),
        2 => Some((Some((bytes.first().copied()? as i8).to_string()), 1)),
        3 => Some((
            Some(i16::from_be_bytes(bytes.get(..2)?.try_into().ok()?).to_string()),
            2,
        )),
        4 => Some((
            Some(i32::from_be_bytes(bytes.get(..4)?.try_into().ok()?).to_string()),
            4,
        )),
        5 | 8 => Some((
            Some(i64::from_be_bytes(bytes.get(..8)?.try_into().ok()?).to_string()),
            8,
        )),
        6 | 7 => {
            let len = u16::from_be_bytes(bytes.get(..2)?.try_into().ok()?) as usize;
            let value = bytes.get(2..2 + len)?;
            let value = (kind == 7)
                .then(|| std::str::from_utf8(value).ok().map(str::to_owned))
                .flatten();
            Some((value, 2 + len))
        }
        9 => bytes.get(..16).map(|_| (None, 16)),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn build_frame(event_type: &str, payload: &[u8]) -> Vec<u8> {
    build_frame_with_headers(&[(":event-type", event_type)], payload)
}

#[cfg(test)]
pub(crate) fn build_frame_with_headers(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
    let mut encoded_headers = Vec::new();
    for (name, value) in headers {
        encoded_headers.push(name.len() as u8);
        encoded_headers.extend_from_slice(name.as_bytes());
        encoded_headers.push(7);
        encoded_headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        encoded_headers.extend_from_slice(value.as_bytes());
    }
    let total_len = 12 + encoded_headers.len() + payload.len() + 4;
    let mut frame = Vec::with_capacity(total_len);
    frame.extend_from_slice(&(total_len as u32).to_be_bytes());
    frame.extend_from_slice(&(encoded_headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&0u32.to_be_bytes());
    frame.extend_from_slice(&encoded_headers);
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&0u32.to_be_bytes());
    frame
}

#[cfg(test)]
#[path = "aws_eventstream_tests.rs"]
mod tests;
