//! SSE framing used by Gemini streaming responses.

use gproxy_channel_api::ChannelError;
use gproxy_protocol::gemini::GenerateContentResponse;

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub(super) fn push(
        &mut self,
        chunk: &[u8],
    ) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > MAX_FRAME_BYTES {
            return Err(ChannelError::Decode(
                "Gemini SSE frame exceeds 16 MiB".into(),
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

    pub(super) fn finish(&mut self) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let raw = std::mem::take(&mut self.buffer);
        Ok(parse(&raw)?.into_iter().collect())
    }
}

fn parse(raw: &[u8]) -> Result<Option<GenerateContentResponse>, ChannelError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| ChannelError::Decode("Gemini SSE frame is not UTF-8".into()))?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|value| value.strip_prefix(' ').unwrap_or(value))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&data.join("\n"))
        .map(Some)
        .map_err(|error| ChannelError::Decode(format!("Gemini SSE event JSON: {error}")))
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
