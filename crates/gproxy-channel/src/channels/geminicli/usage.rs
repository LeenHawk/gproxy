//! Per-call metering. Both the buffered reply and the `alt=sse` stream
//! carry the Gemini `usageMetadata` under `response`; the shared Code Assist
//! observers unwrap it before reading (v3 `geminicli/usage.rs`).

use super::GeminiCli;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext, UsageTransport,
};
use crate::channels::shared::code_assist;
use gproxy_protocol::connection::StreamFraming;

impl UsageExtractor for GeminiCli {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        code_assist::usage::buffered(&context)
    }
}

impl UsageStream for GeminiCli {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse) | None,
            } => Ok(Box::new(code_assist::usage::SseUsageObserver::new())),
            _ => Err(ChannelError::InvalidResponse(
                "Code Assist streams are SSE".into(),
            )),
        }
    }
}
