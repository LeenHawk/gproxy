use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::Value;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        return crate::shared::claude::usage::from_body(ctx.response_body);
    }
    let response = ctx.response_body;
    let mut usage = crate::shared::openai::usage_from_body(ctx)?;
    let value: Value = serde_json::from_slice(response).ok()?;
    if let Some(cached) = value
        .get("usage")
        .and_then(|usage| usage.get("cached_tokens"))
        .and_then(Value::as_u64)
    {
        usage.cached_input_tokens = cached;
    }
    Some(usage)
}

pub(super) fn cached(value: &Value) -> Option<u64> {
    value.get("cached_tokens").and_then(Value::as_u64)
}
