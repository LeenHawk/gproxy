use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if matches!(
        ctx.key.operation(),
        Operation::CreateImage | Operation::EditImage
    ) {
        return image(ctx);
    }
    crate::shared::openai::usage_from_body(ctx)
}

fn image(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(ctx.response_body).ok()?;
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let inner = value.get("data")?;
    let encoded = serde_json::to_vec(inner).ok()?;
    let mut usage = crate::shared::openai::usage_from_body(UsageCtx {
        key: ctx.key,
        request_body: ctx.request_body,
        response_headers: ctx.response_headers,
        response_body: &encoded,
    })
    .unwrap_or_default();
    let count = u64::try_from(inner.get("data").and_then(Value::as_array)?.len()).ok()?;
    usage
        .metrics
        .insert("image_outputs".into(), Decimal::from(count));
    Some(usage)
}
