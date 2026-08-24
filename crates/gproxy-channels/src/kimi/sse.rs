use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::Value;

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    if ctx.key.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        return crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>);
    }
    let chat = ctx.key.kind == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat);
    let inner = crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)?;
    if chat {
        Some(Box::new(KimiChatDecoder {
            inner,
            buffer: Vec::new(),
            cached: None,
        }))
    } else {
        Some(Box::new(inner))
    }
}

struct KimiChatDecoder {
    inner: crate::shared::openai::OpenAiSseDecoder,
    buffer: Vec<u8>,
    cached: Option<u64>,
}

impl KimiChatDecoder {
    fn drain(&mut self) {
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            self.observe(&raw[..end]);
        }
    }

    fn observe(&mut self, raw: &[u8]) {
        let Some(data) = frame_data(raw) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if let Some(cached) = value.get("usage").and_then(super::usage::cached) {
            self.cached = Some(cached);
        }
    }
}

impl StreamDecoder for KimiChatDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "Kimi Chat SSE frame exceeds 16 MiB".into(),
            ));
        }
        self.drain();
        self.inner.push(chunk)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Complete && !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            self.observe(&raw);
        } else {
            self.buffer.clear();
        }
        let mut tail = self.inner.finish(end)?;
        if let (Some(usage), Some(cached)) = (tail.usage.as_mut(), self.cached) {
            usage.cached_input_tokens = cached;
        }
        Ok(tail)
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

fn frame_data(raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|value| value.strip_prefix(' ').unwrap_or(value))
        .collect::<Vec<_>>();
    (!data.is_empty()).then(|| data.join("\n"))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|value| value == needle)
}
