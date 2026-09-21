//! Usage from whichever of the three compatible shapes the surface answered
//! in. OpenCode resells other vendors' models and adds no field of its own,
//! so the shared reader does the whole job (v3 `opencode/usage.rs`, which
//! likewise dispatched on the dialect alone).

use super::OpenCode;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::usage;
use serde_json::Value;

fn enrich(_: &Value, _: &Value, _: &mut NormalizedUsage) {}

impl UsageExtractor for OpenCode {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage::from_body(
            ctx.operation.dialect,
            ctx.response.body,
            enrich,
        ))
    }
}

impl UsageStream for OpenCode {
    fn start(&self, ctx: UsageStreamContext<'_>) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(usage::observer(
            ctx.operation.dialect,
            ctx.transport,
            enrich,
        ))
    }
}
