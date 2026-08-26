use std::sync::Arc;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamDecoder, StreamEnd, StreamTail};

use super::{CompiledRule, RuleModels, apply_response};

pub struct ResponseRuleDecoder {
    upstream: Option<Box<dyn StreamDecoder>>,
    buffer: Vec<u8>,
    rules: Arc<[CompiledRule]>,
    operation: gproxy_protocol::OperationKey,
    primary_model: String,
    alternate_model: Option<String>,
    client_headers: http::HeaderMap,
}

impl ResponseRuleDecoder {
    pub fn new(
        upstream: Option<Box<dyn StreamDecoder>>,
        rules: Arc<[CompiledRule]>,
        operation: gproxy_protocol::OperationKey,
        models: RuleModels<'_>,
        client_headers: http::HeaderMap,
    ) -> Self {
        let (primary_model, alternate_model) = models.owned();
        Self {
            upstream,
            buffer: Vec::new(),
            rules,
            operation,
            primary_model,
            alternate_model,
            client_headers,
        }
    }

    fn rewrite(&self, frame: SseFrame) -> Frame {
        if frame.data.trim() == "[DONE]" {
            return Frame(frame.encode());
        }
        let body = apply_response(
            &self.rules,
            self.operation,
            RuleModels::new(&self.primary_model, self.alternate_model.as_deref()),
            &self.client_headers,
            Bytes::from(frame.data),
        );
        let data = String::from_utf8(body.to_vec()).unwrap_or_default();
        Frame(
            SseFrame {
                event: frame.event,
                data,
            }
            .encode(),
        )
    }

    fn decode(&mut self, frames: Vec<Frame>) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for frame in frames {
            self.buffer.extend_from_slice(&frame.0);
            if self.buffer.len() > 100 * 1024 * 1024 {
                return Err(ChannelError::Decode(
                    "process SSE frame exceeds 100 MiB".into(),
                ));
            }
            while let Some((end, delimiter)) = delimiter(&self.buffer) {
                let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
                if let Some(frame) = SseFrame::parse(&raw[..end])? {
                    output.push(self.rewrite(frame));
                }
            }
        }
        Ok(output)
    }
}

impl StreamDecoder for ResponseRuleDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let frames = match self.upstream.as_mut() {
            Some(upstream) => upstream.push(chunk)?,
            None => vec![Frame(chunk)],
        };
        self.decode(frames)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let mut tail = match self.upstream.as_mut() {
            Some(upstream) => upstream.finish(end)?,
            None => StreamTail::default(),
        };
        if end == StreamEnd::Interrupted {
            self.buffer.clear();
            tail.frames.clear();
            return Ok(tail);
        }
        let mut frames = self.decode(std::mem::take(&mut tail.frames))?;
        if !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            if let Some(frame) = SseFrame::parse(&raw)? {
                frames.push(self.rewrite(frame));
            }
        }
        tail.frames = frames;
        Ok(tail)
    }
}

struct SseFrame {
    event: Option<String>,
    data: String,
}

impl SseFrame {
    fn parse(raw: &[u8]) -> Result<Option<Self>, ChannelError> {
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
        Ok((!data.is_empty()).then(|| Self {
            event,
            data: data.join("\n"),
        }))
    }

    fn encode(self) -> Bytes {
        let mut output = String::new();
        if let Some(event) = self.event {
            output.push_str("event: ");
            output.push_str(&event);
            output.push('\n');
        }
        for line in self.data.lines() {
            output.push_str("data: ");
            output.push_str(line);
            output.push('\n');
        }
        output.push('\n');
        Bytes::from(output)
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

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
