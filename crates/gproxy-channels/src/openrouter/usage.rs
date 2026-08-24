use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr as _;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    let operation = ctx.key.operation;
    let response = ctx.response_body;
    let base = if matches!(
        ctx.key.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    ) {
        crate::shared::claude::usage::from_body(response)
    } else {
        crate::shared::openai::usage_from_body(ctx)
    };
    let Ok(value) = serde_json::from_slice::<Value>(response) else {
        return base;
    };
    let usage = value.get("usage");
    let mut normalized = enrich(base, usage);
    if operation == Operation::Rerank {
        normalized = rerank(normalized, usage);
    }
    if let Some(is_byok) = value
        .pointer("/openrouter_metadata/is_byok")
        .and_then(Value::as_bool)
        && let Some(usage) = normalized.as_mut()
    {
        usage
            .dimensions
            .insert("is_byok".into(), is_byok.to_string());
    }
    normalized
}

pub(super) fn enrich(
    base: Option<NormalizedUsage>,
    usage: Option<&Value>,
) -> Option<NormalizedUsage> {
    let mut measured = base.is_some();
    let mut normalized = base.unwrap_or_default();
    let Some(usage) = usage else {
        return measured.then_some(normalized);
    };
    if let Some(cost) = usage.get("cost").and_then(decimal) {
        normalized.metrics.insert("upstream_cost".into(), cost);
        measured = true;
    }
    if let Some(details) = usage.get("cost_details").and_then(Value::as_object) {
        for name in [
            "upstream_inference_cost",
            "upstream_inference_input_cost",
            "upstream_inference_output_cost",
            "upstream_inference_prompt_cost",
            "upstream_inference_completions_cost",
        ] {
            if let Some(value) = details.get(name).and_then(decimal) {
                normalized.metrics.insert(name.into(), value);
                measured = true;
            }
        }
    }
    if let Some(tokens) = usage
        .get("prompt_tokens_details")
        .and_then(|details| details.get("video_tokens"))
        .and_then(Value::as_u64)
    {
        normalized
            .metrics
            .insert("video_input_tokens".into(), Decimal::from(tokens));
        measured = true;
    }
    if let Some(is_byok) = usage.get("is_byok").and_then(Value::as_bool) {
        normalized
            .dimensions
            .insert("is_byok".into(), is_byok.to_string());
        measured = true;
    }
    measured.then_some(normalized)
}

fn rerank(base: Option<NormalizedUsage>, usage: Option<&Value>) -> Option<NormalizedUsage> {
    let mut measured = base.is_some();
    let mut normalized = base.unwrap_or_default();
    if normalized.input_tokens == 0
        && let Some(tokens) = usage
            .and_then(|usage| usage.get("total_tokens"))
            .and_then(Value::as_u64)
    {
        normalized.input_tokens = tokens;
        measured = true;
    }
    if let Some(units) = usage
        .and_then(|usage| usage.get("search_units"))
        .and_then(Value::as_u64)
    {
        normalized
            .metrics
            .insert("search_units".into(), Decimal::from(units));
        measured = true;
    }
    measured.then_some(normalized)
}

fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(value) => Decimal::from_str(&value.to_string()).ok(),
        Value::String(value) => Decimal::from_str(value).ok(),
        _ => None,
    }
}
