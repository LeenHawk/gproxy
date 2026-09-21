//! Chat Completions usage. The Copilot backend answers OpenAI's own shape
//! and adds no field of its own, so the shared reader does the whole job
//! (v3 `copilotcli/usage.rs`, which likewise deferred to the OpenAI reader).

use super::CopilotCli;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::usage;
use serde_json::Value;

fn enrich(_: &Value, _: &Value, _: &mut NormalizedUsage) {}

impl UsageExtractor for CopilotCli {
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

impl UsageStream for CopilotCli {
    fn start(&self, ctx: UsageStreamContext<'_>) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(usage::observer(
            ctx.operation.dialect,
            ctx.transport,
            enrich,
        ))
    }
}
