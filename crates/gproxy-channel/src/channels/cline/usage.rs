//! Chat Completions usage, read through Cline's envelope.
//!
//! A buffered reply keeps its usage object inside `data`, so the envelope is
//! removed before the shared Chat Completions reader sees it (v3
//! `cline/usage.rs`). The streamed reply is ordinary SSE and carries no
//! envelope, so the observer watches it directly.

use super::Cline;
use super::response::unwrap_envelope;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::usage;
use serde_json::Value;

/// Cline resells other vendors' models and adds nothing of its own to the
/// usage object.
fn enrich(_: &Value, _: &Value, _: &mut NormalizedUsage) {}

impl UsageExtractor for Cline {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(match unwrap_envelope(ctx.response.body)? {
            Some(data) => usage::from_root(ctx.operation.dialect, &data, enrich),
            None => usage::from_body(ctx.operation.dialect, ctx.response.body, enrich),
        })
    }
}

impl UsageStream for Cline {
    fn start(&self, ctx: UsageStreamContext<'_>) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(usage::observer(
            ctx.operation.dialect,
            ctx.transport,
            enrich,
        ))
    }
}
