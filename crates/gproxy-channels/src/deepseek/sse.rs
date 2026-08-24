use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::{ContentGenerationKind, OperationKind, WireFamily};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    match ctx.key.kind {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
            Some(Box::new(ChatDecoder::new()))
        }
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => {
            crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent,
        )
        | OperationKind::Family(WireFamily::OpenAi | WireFamily::Claude | WireFamily::Gemini) => {
            None
        }
    }
}

struct ChatDecoder {
    buffer: Vec<u8>,
    usage: Option<gproxy_channel_api::NormalizedUsage>,
}

impl ChatDecoder {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            usage: None,
        }
    }

    fn drain(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            frames.push(self.rewrite(raw, end));
        }
        frames
    }

    fn rewrite(&mut self, raw: Vec<u8>, event_end: usize) -> Frame {
        let Some(range) = data_range(&raw[..event_end]) else {
            return Frame(Bytes::from(raw));
        };
        let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&raw[range.clone()]) else {
            return Frame(Bytes::from(raw));
        };
        if let Some(usage) = value.get("usage").and_then(super::usage::from_chat_usage) {
            self.usage = Some(usage);
        }
        if !super::shape::normalize_response_value(&mut value) {
            return Frame(Bytes::from(raw));
        }
        let Ok(payload) = serde_json::to_vec(&value) else {
            return Frame(Bytes::from(raw));
        };
        let mut output = Vec::with_capacity(raw.len() + payload.len());
        output.extend_from_slice(&raw[..range.start]);
        output.extend_from_slice(&payload);
        output.extend_from_slice(&raw[range.end..]);
        Frame(Bytes::from(output))
    }
}

impl StreamDecoder for ChatDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        let frames = self.drain();
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "DeepSeek Chat SSE event exceeds 16 MiB".into(),
            ));
        }
        Ok(frames)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let frames = if end == StreamEnd::Complete && !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            let end = raw.len();
            vec![self.rewrite(raw, end)]
        } else {
            self.buffer.clear();
            Vec::new()
        };
        Ok(StreamTail {
            frames,
            usage: self.usage.take(),
        })
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

fn data_range(raw: &[u8]) -> Option<std::ops::Range<usize>> {
    let mut offset = 0;
    let mut found = None;
    while offset < raw.len() {
        let line_end = raw[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(raw.len(), |end| offset + end);
        let content_end = line_end - usize::from(line_end > offset && raw[line_end - 1] == b'\r');
        let line = raw.get(offset..content_end)?;
        if let Some(mut start) = line.strip_prefix(b"data:").map(|_| offset + 5) {
            if found.is_some() {
                return None;
            }
            if raw.get(start) == Some(&b' ') {
                start += 1;
            }
            found = Some(start..content_end);
        }
        offset = line_end.saturating_add(1);
    }
    found
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
