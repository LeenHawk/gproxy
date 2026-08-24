use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr as _;

pub(crate) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    let operation = ctx.key.operation;
    let response = ctx.response_body;
    let base = crate::shared::openai::usage_from_body(ctx);
    let Ok(value) = serde_json::from_slice::<Value>(response) else {
        return base;
    };
    enrich(base, value.get("usage"), Some(&value), operation)
}

pub(super) fn enrich_stream(
    base: Option<NormalizedUsage>,
    usage: Option<&Value>,
) -> Option<NormalizedUsage> {
    enrich(base, usage, None, Operation::GenerateContent)
}

fn enrich(
    base: Option<NormalizedUsage>,
    usage: Option<&Value>,
    root: Option<&Value>,
    operation: Operation,
) -> Option<NormalizedUsage> {
    let mut measured = base.is_some();
    let mut normalized = base.unwrap_or_default();
    if let Some(usage) = usage {
        measured |= metric(
            &mut normalized,
            "cost_in_usd_ticks",
            usage.get("cost_in_usd_ticks"),
        );
        measured |= metric(
            &mut normalized,
            "image_input_tokens",
            usage.pointer("/input_tokens_details/image_tokens"),
        );
        if let Some(searches) = usage
            .get("server_side_tool_usage_details")
            .and_then(|details| details.get("web_search_requests"))
            .and_then(Value::as_u64)
        {
            normalized
                .metrics
                .insert("web_searches".into(), Decimal::from(searches));
            measured = true;
        }
    }
    if matches!(
        operation,
        Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::EditVideo
            | Operation::ExtendVideo
    ) && let Some(root) = root
    {
        measured |= metric(
            &mut normalized,
            "upstream_cost",
            root.get("cost_usd").or_else(|| root.get("cost")),
        );
        measured |= metric(
            &mut normalized,
            "video_seconds",
            root.get("duration").or_else(|| root.get("seconds")),
        );
    }
    measured.then_some(normalized)
}

fn metric(usage: &mut NormalizedUsage, name: &str, value: Option<&Value>) -> bool {
    let Some(value) = value.and_then(decimal) else {
        return false;
    };
    usage.metrics.insert(name.into(), value);
    true
}

fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(value) => Decimal::from_str(&value.to_string()).ok(),
        Value::String(value) => Decimal::from_str(value).ok(),
        _ => None,
    }
}
