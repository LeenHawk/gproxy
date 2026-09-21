//! Per-call metering. Both the buffered reply and the `alt=sse` stream
//! carry the Gemini `usageMetadata` under `response`; the shared Code Assist
//! observers unwrap it before reading (v3 `antigravity/usage.rs`).

use super::Antigravity;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext, UsageTransport,
};
use crate::channels::shared::code_assist;
use gproxy_protocol::connection::StreamFraming;

impl UsageExtractor for Antigravity {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        code_assist::usage::buffered(&context)
    }
}

impl UsageStream for Antigravity {
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
