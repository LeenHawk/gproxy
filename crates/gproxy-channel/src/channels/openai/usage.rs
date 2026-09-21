//! Per-call metering for the OpenAI platform. The shapes are the reference
//! ones, so the reading itself lives in `channels::shared::openai_wire`; this
//! module only says which dialect a call used.

use super::OpenAi;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext, UsageTransport,
};
use crate::channels::shared::openai_wire;
use gproxy_protocol::connection::StreamFraming;

impl UsageExtractor for OpenAi {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(openai_wire::extract(&context))
    }
}

impl UsageStream for OpenAi {
    /// Only SSE is watched. A Responses WebSocket delivers its own frames,
    /// which the host hands to the envelope layer rather than to a channel
    /// observer, so this reports nothing for it rather than pretending.
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse) | None,
            } => openai_wire::observer(context.operation.operation, context.operation.dialect),
            _ => Err(ChannelError::InvalidResponse(
                "OpenAI streams are SSE".into(),
            )),
        }
    }
}
