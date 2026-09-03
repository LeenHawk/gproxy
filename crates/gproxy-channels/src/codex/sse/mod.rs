mod event;
mod framing;
mod lifecycle;
mod tools;

#[cfg(test)]
mod tests;

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, Frame, NormalizedUsage, StreamCtx, StreamDecoder, StreamEnd, StreamTail,
};
use gproxy_protocol::openai::generate_content::responses::ResponseStreamEvent;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};

use framing::{delimiter, encode, parse};

pub(super) struct CodexSseDecoder {
    buffer: Vec<u8>,
    lifecycle: lifecycle::Lifecycle,
    tools: tools::ToolAliases,
    usage: Option<NormalizedUsage>,
    actual_service_tier: Option<String>,
    done_seen: bool,
}

impl CodexSseDecoder {
    pub(super) fn for_operation(ctx: StreamCtx<'_>) -> Option<Self> {
        (ctx.key.operation() == Operation::StreamGenerateContent
            && ctx.key.kind()
                == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses))
        .then(|| Self {
            buffer: Vec::new(),
            lifecycle: Default::default(),
            tools: Default::default(),
            usage: None,
            actual_service_tier: None,
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
                    && let Some(response) = event::response(known)
                {
                    if let Some(tier) = response.service_tier.as_ref() {
                        self.actual_service_tier = Some(tier.as_str().into());
                    }
                    if let Some(usage) = response.usage.as_ref() {
                        self.usage = Some(super::usage::from_response_with_tier(
                            usage,
                            response.service_tier.as_ref(),
                        ));
                    }
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
        if self.buffer.len() > 100 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "Codex Responses SSE frame exceeds 100 MiB".into(),
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
                actual_service_tier: self.actual_service_tier.take(),
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
            actual_service_tier: self.actual_service_tier.take(),
        })
    }
}
