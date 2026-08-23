mod from_claude;
mod to_claude;

use bytes::Bytes;
use gproxy_protocol::OperationKey;

use crate::TransformError;
use crate::pair::{self, Pair};

pub struct ResponseStream {
    decoder: SseDecoder,
    converter: Converter,
}

enum Converter {
    FromClaude(from_claude::Converter),
    ToClaude(to_claude::Converter),
}

impl ResponseStream {
    pub fn new(source: OperationKey, target: OperationKey) -> Result<Self, TransformError> {
        let pair = pair::resolve(source, target).ok_or(TransformError::UnsupportedPair {
            source_key: source,
            target_key: target,
        })?;
        let converter = match pair {
            Pair::ChatToClaude => {
                Converter::FromClaude(from_claude::Converter::new(from_claude::Output::Chat))
            }
            Pair::ResponsesToClaude => {
                Converter::FromClaude(from_claude::Converter::new(from_claude::Output::Responses))
            }
            Pair::ClaudeToChat => {
                Converter::ToClaude(to_claude::Converter::new(to_claude::Input::Chat))
            }
            Pair::ClaudeToResponses => {
                Converter::ToClaude(to_claude::Converter::new(to_claude::Input::Responses))
            }
            _ => {
                return Err(TransformError::UnsupportedPair {
                    source_key: source,
                    target_key: target,
                });
            }
        };
        Ok(Self {
            decoder: SseDecoder::default(),
            converter,
        })
    }

    pub fn push(&mut self, chunk: Bytes) -> Result<Vec<Bytes>, TransformError> {
        let frames = self.decoder.push(&chunk)?;
        let mut output = Vec::new();
        for frame in frames {
            output.extend(self.convert(frame)?);
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if let Some(frame) = self.decoder.finish()? {
            output.extend(self.convert(frame)?);
        }
        match &mut self.converter {
            Converter::FromClaude(converter) => output.extend(converter.finish()?),
            Converter::ToClaude(converter) => output.extend(converter.finish()?),
        }
        Ok(output)
    }

    fn convert(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        match &mut self.converter {
            Converter::FromClaude(converter) => converter.frame(frame),
            Converter::ToClaude(converter) => converter.frame(frame),
        }
    }
}

#[derive(Debug)]
pub(super) struct SseFrame {
    pub event: Option<String>,
    pub data: String,
}

impl SseFrame {
    pub(super) fn json(event: Option<&str>, value: serde_json::Value) -> Bytes {
        Self::encode(event, &value.to_string())
    }

    pub(super) fn encode(event: Option<&str>, data: &str) -> Bytes {
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
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, TransformError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(TransformError::shape("SSE", "frame exceeds 16 MiB"));
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

    fn finish(&mut self) -> Result<Option<SseFrame>, TransformError> {
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
        event,
        data: data.join("\n"),
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
