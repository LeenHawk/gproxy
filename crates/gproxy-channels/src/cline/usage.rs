use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use serde_json::Value;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(ctx.response_body).ok()?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return crate::shared::openai::usage_from_body(ctx);
    }
    let body = serde_json::to_vec(value.get("data")?).ok()?;
    crate::shared::openai::usage_from_body(UsageCtx {
        key: ctx.key,
        request_body: ctx.request_body,
        response_headers: ctx.response_headers,
        response_body: &body,
    })
}
