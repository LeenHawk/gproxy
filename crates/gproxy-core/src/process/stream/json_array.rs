use bytes::Bytes;
use gproxy_channel_api::ChannelError;

use super::EncodedFrame;

#[derive(Default)]
pub(super) struct JsonArrayCodec {
    buffer: Vec<u8>,
    started: bool,
    emitted: bool,
    ended: bool,
}

impl JsonArrayCodec {
    pub(super) fn push(&mut self, chunk: Bytes) -> Result<Vec<EncodedFrame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 100 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "process JSON-array element exceeds 100 MiB".into(),
            ));
        }
        self.decode()
    }

    pub(super) fn finish(&mut self) -> Result<Vec<EncodedFrame>, ChannelError> {
        let mut output = self.decode()?;
        if !self.ended || !self.buffer.iter().all(u8::is_ascii_whitespace) {
            return Err(ChannelError::Decode(
                "process JSON-array stream ended mid-element".into(),
            ));
        }
        output.push(EncodedFrame::Raw(if self.emitted {
            Bytes::from_static(b"]")
        } else {
            Bytes::from_static(b"[]")
        }));
        Ok(output)
    }

    fn decode(&mut self) -> Result<Vec<EncodedFrame>, ChannelError> {
        let mut output = Vec::new();
        if !self.started {
            trim(&mut self.buffer);
            if self.buffer.first() != Some(&b'[') {
                return Ok(output);
            }
            self.buffer.remove(0);
            self.started = true;
        }
        loop {
            trim(&mut self.buffer);
            if self.buffer.first() == Some(&b',') {
                self.buffer.remove(0);
                trim(&mut self.buffer);
            }
            if self.buffer.first() == Some(&b']') {
                self.buffer.remove(0);
                self.ended = true;
                break;
            }
            let Some(end) = value_end(&self.buffer) else {
                break;
            };
            let data = Bytes::from(self.buffer.drain(..end).collect::<Vec<_>>());
            let prefix = if self.emitted {
                b",".as_slice()
            } else {
                b"[".as_slice()
            };
            self.emitted = true;
            output.push(EncodedFrame::Json { prefix, data });
        }
        Ok(output)
    }
}

fn trim(buffer: &mut Vec<u8>) {
    let count = buffer
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    buffer.drain(..count);
}

fn value_end(buffer: &[u8]) -> Option<usize> {
    if !matches!(buffer.first(), Some(b'{') | Some(b'[')) {
        return None;
    }
    let mut depth = 0_usize;
    let mut string = false;
    let mut escaped = false;
    for (index, byte) in buffer.iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}
