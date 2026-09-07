use gproxy_channel_api::NormalizedUsage;
use rust_decimal::Decimal;
use serde_json::Value;

mod attempts;
pub(super) use attempts::attach;

pub(crate) fn from_body(body: &[u8]) -> Option<NormalizedUsage> {
    let body = serde_json::from_slice::<Value>(body).ok()?;
    let wire = body.get("usage")?;
    let mut usage = from_usage(wire)?;
    attach(
        &mut usage,
        wire,
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        body["stop_reason"] == "refusal",
    );
    Some(usage)
}

pub(crate) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    if !number(usage, "input_tokens") || !number(usage, "output_tokens") {
        return None;
    }
    let cache_read = field(usage, "cache_read_input_tokens");
    let mut normalized = NormalizedUsage {
        input_tokens: field(usage, "input_tokens").saturating_add(cache_read),
        output_tokens: field(usage, "output_tokens"),
        cached_input_tokens: cache_read,
        ..Default::default()
    };
    let (cache_5m, cache_1h) = usage
        .get("cache_creation")
        .filter(|value| value.is_object())
        .map(|cache| {
            (
                field(cache, "ephemeral_5m_input_tokens"),
                field(cache, "ephemeral_1h_input_tokens"),
            )
        })
        .unwrap_or_else(|| (field(usage, "cache_creation_input_tokens"), 0));
    add_metric(&mut normalized, "cache_creation_5m_tokens", cache_5m);
    add_metric(&mut normalized, "cache_creation_1h_tokens", cache_1h);
    add_metric(
        &mut normalized,
        "reasoning_tokens",
        usage
            .get("output_tokens_details")
            .map(|details| field(details, "thinking_tokens"))
            .unwrap_or_default(),
    );
    let tools = usage.get("server_tool_use");
    add_metric(
        &mut normalized,
        "web_searches",
        tools
            .map(|tools| field(tools, "web_search_requests"))
            .unwrap_or_default(),
    );
    add_metric(
        &mut normalized,
        "web_fetches",
        tools
            .map(|tools| field(tools, "web_fetch_requests"))
            .unwrap_or_default(),
    );
    for (source, target) in [
        ("speed", "speed"),
        ("service_tier", "service_tier"),
        ("inference_geo", "inference_geo"),
    ] {
        if let Some(value) = usage.get(source).and_then(string_value) {
            normalized.dimensions.insert(target.into(), value);
        }
    }
    Some(normalized)
}

pub(crate) fn merge_stream(
    start: Option<&Value>,
    delta: Option<&Value>,
) -> Option<NormalizedUsage> {
    let start_has_input = start.is_some_and(|usage| number(usage, "input_tokens"));
    let delta_has_input = delta.is_some_and(|usage| number(usage, "input_tokens"));
    let delta_has_output = delta.is_some_and(|usage| number(usage, "output_tokens"));
    if !delta_has_output || (!start_has_input && !delta_has_input) {
        return None;
    }
    let mut merged = start
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let delta = delta?.as_object()?;
    for name in [
        "output_tokens",
        "output_tokens_details",
        "server_tool_use",
        "speed",
        "service_tier",
        "inference_geo",
        "iterations",
    ] {
        if let Some(value) = delta.get(name) {
            merged.insert(name.into(), value.clone());
        }
    }
    for name in ["input_tokens", "cache_read_input_tokens"] {
        if delta.get(name).is_some_and(Value::is_u64) {
            merged.insert(name.into(), delta[name].clone());
        }
    }
    let start_has_creation = start.is_some_and(has_cache_creation);
    let delta_has_breakdown = delta.get("cache_creation").is_some_and(Value::is_object);
    if delta_has_breakdown
        || (!start_has_creation && has_cache_creation(&Value::Object(delta.clone())))
    {
        for name in ["cache_creation", "cache_creation_input_tokens"] {
            if let Some(value) = delta.get(name) {
                merged.insert(name.into(), value.clone());
            }
        }
    }
    from_usage(&Value::Object(merged))
}

fn has_cache_creation(usage: &Value) -> bool {
    usage.get("cache_creation").is_some_and(Value::is_object)
        || number(usage, "cache_creation_input_tokens")
}

fn add_metric(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

fn string_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_object()?.get("name")?.as_str().map(str::to_owned))
}

fn field(value: &Value, name: &str) -> u64 {
    value.get(name).and_then(Value::as_u64).unwrap_or_default()
}

fn number(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_u64)
}
