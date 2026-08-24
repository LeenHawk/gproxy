use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::Value;

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    let kind = match ctx.key.kind {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => Kind::Chat,
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => Kind::Responses,
        _ => return None,
    };
    let inner = crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)?;
    Some(Box::new(XaiSseDecoder {
        inner,
        kind,
        buffer: Vec::new(),
        usage: None,
    }))
}

struct XaiSseDecoder {
    inner: crate::shared::openai::OpenAiSseDecoder,
    kind: Kind,
    buffer: Vec<u8>,
    usage: Option<Value>,
}

#[derive(Clone, Copy)]
enum Kind {
    Chat,
    Responses,
}

impl XaiSseDecoder {
    fn drain(&mut self) {
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            self.observe(&raw[..end]);
        }
    }

    fn observe(&mut self, raw: &[u8]) {
        let Some((event, data)) = frame(raw) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        let usage = match self.kind {
            Kind::Chat => value.get("usage"),
            Kind::Responses => {
                let complete = event.as_deref() == Some("response.completed")
                    || value.get("type").and_then(Value::as_str) == Some("response.completed");
                complete.then(|| value.pointer("/response/usage")).flatten()
            }
        };
        if let Some(usage) = usage {
            self.usage = Some(usage.clone());
        }
    }
}

impl StreamDecoder for XaiSseDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode("xAI SSE frame exceeds 16 MiB".into()));
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
        tail.usage = super::usage::enrich_stream(tail.usage, self.usage.as_ref());
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

fn frame(raw: &[u8]) -> Option<(Option<String>, String)> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().into());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    (!data.is_empty()).then(|| (event, data.join("\n")))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|value| value == needle)
}
