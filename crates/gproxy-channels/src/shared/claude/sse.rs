use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use serde_json::Value;

pub(crate) struct ClaudeSseDecoder {
    buffer: Vec<u8>,
    start: Option<Value>,
    delta: Option<Value>,
    model: String,
    refused: bool,
}

impl ClaudeSseDecoder {
    pub(crate) fn for_operation(ctx: StreamCtx<'_>) -> Option<Self> {
        matches!(
            (ctx.key.operation(), ctx.key.kind()),
            (
                Operation::StreamGenerateContent,
                OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
            )
        )
        .then(|| Self {
            buffer: Vec::new(),
            start: None,
            delta: None,
            model: String::new(),
            refused: false,
        })
    }

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
        let Ok(event) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") if self.start.is_none() => {
                self.start = event.pointer("/message/usage").cloned();
                self.model = event
                    .pointer("/message/model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into();
            }
            Some("content_block_start")
                if event.pointer("/content_block/type").and_then(Value::as_str)
                    == Some("fallback") =>
            {
                if let Some(model) = event
                    .pointer("/content_block/to/model")
                    .and_then(Value::as_str)
                {
                    self.model = model.into();
                }
            }
            Some("message_delta")
                if event
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .is_some_and(Value::is_u64) =>
            {
                self.delta = event.get("usage").cloned();
                self.refused =
                    event.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal");
            }
            _ => {}
        }
    }
}

impl StreamDecoder for ClaudeSseDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 100 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "Claude SSE frame exceeds 100 MiB".into(),
            ));
        }
        self.drain();
        if chunk.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![Frame(chunk)])
        }
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Complete && !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            self.observe(&raw);
        } else {
            self.buffer.clear();
        }
        let mut usage = super::usage::merge_stream(self.start.as_ref(), self.delta.as_ref());
        if let Some(usage) = usage.as_mut() {
            super::usage::attach(
                usage,
                self.delta.as_ref().unwrap_or(&Value::Null),
                &self.model,
                self.refused,
            );
        }
        Ok(StreamTail {
            frames: Vec::new(),
            usage,
            actual_service_tier: None,
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
        .position(|candidate| candidate == needle)
}
