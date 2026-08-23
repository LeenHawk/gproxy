use bytes::Bytes;
use gproxy_protocol::OperationKey;

use crate::TransformError;
use crate::registry::{self, TransformPair};

pub use collector::{BufferedResponse, ResponseCollector};

pub(crate) fn is_promotion(source: OperationKey, target: OperationKey) -> bool {
    matches!(
        (source.operation, source.kind, target.operation, target.kind),
        (
            gproxy_protocol::Operation::GenerateContent,
            gproxy_protocol::OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::OpenAiResponses
            ),
            gproxy_protocol::Operation::StreamGenerateContent,
            gproxy_protocol::OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::OpenAiResponses
            )
        )
    )
}

pub(crate) fn promotion_request(body: Bytes) -> Result<Bytes, TransformError> {
    let _: gproxy_protocol::openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    Ok(body)
}

pub(crate) fn promotion_response(body: Bytes) -> Result<Bytes, TransformError> {
    let _: gproxy_protocol::openai::ResponseObject = serde_json::from_slice(&body)?;
    Ok(body)
}

pub struct ResponseStream {
    decoder: SseDecoder,
    converter: Box<dyn Converter>,
}

pub(crate) trait Converter: Send {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError>;
    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError>;
}

impl ResponseStream {
    pub fn new(source: OperationKey, target: OperationKey) -> Result<Self, TransformError> {
        let pair = registry::resolve(source, target).ok_or(TransformError::UnsupportedPair {
            source_key: source,
            target_key: target,
        })?;
        let converter = match pair {
            TransformPair::ChatToClaude => {
                crate::generate_content::openai_chat_to_claude_messages::stream::converter()
            }
            TransformPair::ResponsesToClaude => {
                crate::generate_content::openai_responses_to_claude_messages::stream::converter()
            }
            TransformPair::ClaudeToChat => {
                crate::generate_content::claude_messages_to_openai_chat::stream::converter()
            }
            TransformPair::ClaudeToResponses => {
                crate::generate_content::claude_messages_to_openai_responses::stream::converter()
            }
            TransformPair::OpenAiChatToResponses => {
                crate::generate_content::openai_chat_to_openai_responses::stream::converter()
            }
            TransformPair::OpenAiResponsesToChat => {
                crate::generate_content::openai_responses_to_openai_chat::stream::converter()
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
        output.extend(self.converter.finish()?);
        Ok(output)
    }

    fn convert(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.converter.frame(frame)
    }
}

#[derive(Debug)]
pub(crate) struct SseFrame {
    pub _event: Option<String>,
    pub data: String,
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
        _event: event,
        data: data.join("\n"),
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
mod collector;
