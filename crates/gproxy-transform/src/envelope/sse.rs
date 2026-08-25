use bytes::Bytes;

use crate::TransformError;

#[derive(Debug)]
pub(crate) struct SseFrame {
    pub(crate) _event: Option<String>,
    pub(crate) data: String,
}

impl SseFrame {
    pub(crate) fn typed<T: serde::Serialize>(
        event: Option<&str>,
        value: &T,
    ) -> Result<Bytes, TransformError> {
        Ok(Self::encode(event, &serde_json::to_string(value)?))
    }

    pub(crate) fn encode(event: Option<&str>, data: &str) -> Bytes {
        let mut output = String::new();
        if let Some(event) = event {
            output.push_str("event: ");
            output.push_str(event);
            output.push('\n');
        }
        for line in data.lines() {
            output.push_str("data: ");
            output.push_str(line);
            output.push('\n');
        }
        output.push('\n');
        Bytes::from(output)
    }
}

#[derive(Default)]
pub(crate) struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, TransformError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > 100 * 1024 * 1024 {
            return Err(TransformError::shape("SSE", "frame exceeds 100 MiB"));
        }
        let mut frames = Vec::new();
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            if let Some(frame) = parse(&raw[..end])? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    pub(crate) fn finish(&mut self) -> Result<Option<SseFrame>, TransformError> {
        if self.buffer.is_empty() {
            return Ok(None);
        }
        let raw = std::mem::take(&mut self.buffer);
        parse(&raw)
    }
}

fn delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (left, right) => left.or(right),
    }
}

fn parse(raw: &[u8]) -> Result<Option<SseFrame>, TransformError> {
    let text =
        std::str::from_utf8(raw).map_err(|_| TransformError::shape("SSE", "frame is not UTF-8"))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    Ok((!data.is_empty()).then(|| SseFrame {
        _event: event,
        data: data.join("\n"),
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
