mod headers;

use bytes::{Buf, Bytes, BytesMut};
use gproxy_channel_api::ChannelError;

const PRELUDE_LEN: usize = 12;
const MIN_FRAME_LEN: usize = PRELUDE_LEN + 4;
const MAX_FRAME_LEN: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) message_type: Option<String>,
    pub(crate) event_type: Option<String>,
    pub(crate) exception_type: Option<String>,
    pub(crate) content_type: Option<String>,
    pub(crate) payload: Bytes,
}

#[derive(Debug, Default)]
pub(crate) struct FrameParser {
    pending: BytesMut,
}

impl FrameParser {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, mut chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let mut frames = Vec::new();
        while !chunk.is_empty() {
            if self.pending.is_empty() && chunk.len() >= PRELUDE_LEN {
                let layout = decode_prelude(&chunk[..PRELUDE_LEN])?;
                if chunk.len() >= layout.total_len {
                    let raw = chunk.split_to(layout.total_len);
                    frames.push(decode_frame(raw, layout)?);
                    continue;
                }
            }

            let needed = if self.pending.len() < PRELUDE_LEN {
                PRELUDE_LEN - self.pending.len()
            } else {
                decode_prelude(&self.pending[..PRELUDE_LEN])?.total_len - self.pending.len()
            };
            let take = needed.min(chunk.len());
            self.pending.extend_from_slice(&chunk[..take]);
            chunk.advance(take);

            if self.pending.len() < PRELUDE_LEN {
                continue;
            }
            let layout = decode_prelude(&self.pending[..PRELUDE_LEN])?;
            if self.pending.len() == layout.total_len {
                let raw = self.pending.split_to(layout.total_len).freeze();
                frames.push(decode_frame(raw, layout)?);
            }
        }
        Ok(frames)
    }

    /// Validate a clean EOF. Callers decide whether an interrupted stream is
    /// allowed to retain an incomplete frame.
    pub(crate) fn finish(&self) -> Result<(), ChannelError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        Err(decode(format!(
            "stream ended inside a frame after {} bytes",
            self.pending.len()
        )))
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
    let expected_crc = read_u32(&prelude[8..12]);
    let actual_crc = crc32fast::hash(&prelude[..8]);
    if actual_crc != expected_crc {
        return Err(decode("prelude CRC mismatch"));
    }
    Ok(Layout {
        total_len,
        headers_len,
    })
}

fn decode_frame(raw: Bytes, layout: Layout) -> Result<Frame, ChannelError> {
    let message_end = layout.total_len - 4;
    let expected_crc = read_u32(&raw[message_end..]);
    let actual_crc = crc32fast::hash(&raw[..message_end]);
    if actual_crc != expected_crc {
        return Err(decode("message CRC mismatch"));
    }
    let headers_end = PRELUDE_LEN + layout.headers_len;
    let fields = headers::decode(&raw[PRELUDE_LEN..headers_end])?;
    Ok(Frame {
        message_type: fields.message_type,
        event_type: fields.event_type,
        exception_type: fields.exception_type,
        content_type: fields.content_type,
        payload: raw.slice(headers_end..message_end),
    })
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::Decode(format!("AWS event-stream: {}", message.into()))
}
