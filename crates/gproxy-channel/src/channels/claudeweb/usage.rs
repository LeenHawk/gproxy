//! Per-call usage over the channel's own translated Messages output. claude.ai
//! reports no token counts: `input_tokens` is v3's character estimate of the
//! request and `output_tokens` the same estimate of the answer, so every
//! reading is `Partial`.

use gproxy_protocol::{
    codec::{SseDecoder, SseFrame},
    connection::StreamFraming,
};
use serde_json::Value;

use super::{ClaudeWeb, stream::SSE_LIMITS};
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};

fn normalized(input: Option<u64>, output: Option<u64>) -> Option<NormalizedUsage> {
    input?;
    let mut usage = NormalizedUsage::default();
    usage.tokens.input_tokens = input;
    usage.tokens.output_tokens = output;
    usage.completeness = UsageCompleteness::Partial;
    Some(usage)
}

impl UsageExtractor for ClaudeWeb {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        let Ok(body) = serde_json::from_slice::<Value>(ctx.response.body) else {
            return Ok(None);
        };
        Ok(normalized(
            body.pointer("/usage/input_tokens").and_then(Value::as_u64),
            body.pointer("/usage/output_tokens").and_then(Value::as_u64),
        ))
    }
}

struct MessagesUsageObserver {
    sse: SseDecoder,
    input: Option<u64>,
    output: Option<u64>,
}

impl MessagesUsageObserver {
    fn see(&mut self, data: &str) {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(input) = event
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64)
                {
                    self.input = Some(input);
                }
            }
            Some("message_delta") => {
                if let Some(output) = event
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.output = Some(output);
                }
            }
            _ => {}
        }
    }
}

impl UsageObserver for MessagesUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        if let UsageFrame::HttpChunk(chunk) = frame {
            let frames = self
                .sse
                .push(chunk)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            for frame in frames {
                if let SseFrame::Event(event) = frame {
                    self.see(&event.data);
                }
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<NormalizedUsage> {
        normalized(self.input, self.output)
    }

    fn finish(
        self: Box<Self>,
        _end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(normalized(self.input, self.output))
    }
}

impl UsageStream for ClaudeWeb {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Ok(Box::new(MessagesUsageObserver {
                sse: SseDecoder::new(SSE_LIMITS),
                input: None,
                output: None,
            })),
            _ => Err(ChannelError::InvalidResponse(
                "Claude Web streams are Messages SSE".into(),
            )),
        }
    }
}
