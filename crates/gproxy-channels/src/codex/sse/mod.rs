mod item_state;
mod lifecycle;
mod tools;

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, Frame, NormalizedUsage, StreamCtx, StreamDecoder, StreamEnd, StreamTail,
};
use gproxy_protocol::openai::generate_content::responses::ResponseStreamEvent;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};

pub(super) struct CodexSseDecoder {
    buffer: Vec<u8>,
    lifecycle: lifecycle::Lifecycle,
    tools: tools::ToolAliases,
    usage: Option<NormalizedUsage>,
    done_seen: bool,
}

impl CodexSseDecoder {
    pub(super) fn for_operation(ctx: StreamCtx<'_>) -> Option<Self> {
        (ctx.key.operation == Operation::StreamGenerateContent
            && ctx.key.kind
                == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses))
        .then(|| Self {
            buffer: Vec::new(),
            lifecycle: Default::default(),
            tools: Default::default(),
            usage: None,
            done_seen: false,
        })
    }

    fn drain(&mut self) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            output.extend(self.frame(&raw[..end])?);
        }
        Ok(output)
    }

    fn frame(&mut self, raw: &[u8]) -> Result<Vec<Frame>, ChannelError> {
        let Some(frame) = parse(raw)? else {
            return Ok(Vec::new());
        };
        if frame.data.trim() == "[DONE]" {
            self.done_seen = true;
            return Ok(Vec::new());
        }
        let event: ResponseStreamEvent = serde_json::from_str(&frame.data)
            .map_err(|error| ChannelError::Decode(format!("Responses event JSON: {error}")))?;
        let events = self
            .lifecycle
            .normalize(event)
            .map_err(ChannelError::Decode)?;
        self.emit(events, frame.event.as_deref())
    }

    fn emit(
        &mut self,
        events: Vec<ResponseStreamEvent>,
        fallback_event: Option<&str>,
    ) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for event in events {
            for event in self.tools.normalize(event)? {
                if let ResponseStreamEvent::Known(known) = &event
                    && let Some(response) = known.response.as_ref()
                    && let Some(usage) = response.usage.as_ref()
                {
                    self.usage = Some(super::usage::from_response_with_tier(
                        usage,
                        response.service_tier.as_ref(),
                    ));
                }
                let name = event.event_name().or(fallback_event);
                let data = serde_json::to_string(&event)
                    .map_err(|error| ChannelError::Decode(error.to_string()))?;
                output.push(Frame(encode(name, &data)));
            }
        }
        Ok(output)
    }
}

impl StreamDecoder for CodexSseDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(&chunk);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "Codex Responses SSE frame exceeds 16 MiB".into(),
            ));
        }
        self.drain()
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Interrupted {
            self.buffer.clear();
            return Ok(StreamTail {
                frames: Vec::new(),
                usage: self.usage.take(),
            });
        }
        let mut frames = if self.buffer.is_empty() {
            Vec::new()
        } else {
            let raw = std::mem::take(&mut self.buffer);
            self.frame(&raw)?
        };
        if !self.lifecycle.is_terminal() {
            return Err(ChannelError::Decode(
                "Codex Responses stream ended without a terminal response event".into(),
            ));
        }
        if self.done_seen {
            frames.push(Frame(encode(None, "[DONE]")));
        }
        Ok(StreamTail {
            frames,
            usage: self.usage.take(),
        })
    }
}

struct SseFrame {
    event: Option<String>,
    data: String,
}

fn parse(raw: &[u8]) -> Result<Option<SseFrame>, ChannelError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| ChannelError::Decode("Codex SSE frame is not UTF-8".into()))?;
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

fn encode(event: Option<&str>, data: &str) -> Bytes {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_channel_api::StreamEnd;
    use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn interrupted_finish_never_synthesizes_a_terminal_event() {
        let mut decoder = CodexSseDecoder::for_operation(StreamCtx {
            key: OperationKey::content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            framing: gproxy_protocol::StreamFraming::Sse,
            request_body: &Bytes::new(),
            response_headers: &http::HeaderMap::new(),
        })
        .unwrap();
        decoder
            .push(Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"m1\",\"delta\":\"partial\"}\n\n",
            ))
            .unwrap();
        let tail = decoder.finish(StreamEnd::Interrupted).unwrap();
        assert!(tail.frames.is_empty());

        let mut truncated = CodexSseDecoder::for_operation(StreamCtx {
            key: OperationKey::content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            framing: gproxy_protocol::StreamFraming::Sse,
            request_body: &Bytes::new(),
            response_headers: &http::HeaderMap::new(),
        })
        .unwrap();
        truncated
            .push(Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"m1\",\"content_index\":0,\"delta\":\"partial\"}\n\n",
            ))
            .unwrap();
        assert!(truncated.finish(StreamEnd::Complete).is_err());
    }
}
