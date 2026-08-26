use bytes::Bytes;
use gproxy_channel_api::ChannelError;

use super::EncodedFrame;

#[derive(Default)]
pub(super) struct SseCodec {
    buffer: Vec<u8>,
}

impl SseCodec {
    pub(super) fn push(&mut self, chunk: Bytes) -> Result<Vec<EncodedFrame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 100 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "process SSE frame exceeds 100 MiB".into(),
            ));
        }
        let mut output = Vec::new();
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            if let Some(frame) = parse(&raw[..end])? {
                output.push(frame);
            }
        }
        Ok(output)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<EncodedFrame>, ChannelError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let raw = std::mem::take(&mut self.buffer);
        Ok(parse(&raw)?.into_iter().collect())
    }
}

fn parse(raw: &[u8]) -> Result<Option<EncodedFrame>, ChannelError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| ChannelError::Decode("process SSE frame is not UTF-8".into()))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().into());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    Ok((!data.is_empty()).then(|| EncodedFrame::Sse {
        event,
        data: Bytes::from(data.join("\n")),
    }))
}

pub(super) fn encode(event: Option<&str>, data: Bytes) -> Bytes {
    let mut output = Vec::new();
    if let Some(event) = event {
        output.extend_from_slice(b"event: ");
        output.extend_from_slice(event.as_bytes());
        output.push(b'\n');
    }
    for line in data.split(|byte| *byte == b'\n') {
        output.extend_from_slice(b"data: ");
        output.extend_from_slice(line);
        output.push(b'\n');
    }
    output.push(b'\n');
    Bytes::from(output)
}

fn delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (left, right) => left.or(right),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
