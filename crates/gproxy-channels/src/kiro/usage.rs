use gproxy_channel_api::NormalizedUsage;
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) fn from_body(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value
        .get("tokenUsage")
        .or_else(|| value.get("usage"))
        .and_then(from_value)
}

pub(super) fn from_value(value: &Value) -> Option<NormalizedUsage> {
    let output = number(
        value,
        &[
            "outputTokens",
            "completionTokens",
            "totalOutputTokens",
            "output_tokens",
        ],
    );
    let cached = number(value, &["cacheReadInputTokens", "cache_read_input_tokens"]);
    let written = number(
        value,
        &[
            "cacheWriteInputTokens",
            "cacheCreationInputTokens",
            "cache_write_input_tokens",
        ],
    );
    let uncached = number(value, &["uncachedInputTokens", "uncached_input_tokens"]);
    let explicit_input = number(
        value,
        &[
            "inputTokens",
            "promptTokens",
            "totalInputTokens",
            "input_tokens",
        ],
    );
    let total = number(value, &["totalTokens", "total_tokens"]);
    if [output, cached, written, uncached, explicit_input, total]
        .iter()
        .all(Option::is_none)
    {
        return None;
    }
    let output = output.unwrap_or_default();
    let input = explicit_input.unwrap_or_else(|| {
        let components =
            uncached.unwrap_or_default() + cached.unwrap_or_default() + written.unwrap_or_default();
        if components > 0 {
            components
        } else {
            total.unwrap_or_default().saturating_sub(output)
        }
    });
    let mut usage = NormalizedUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached.unwrap_or_default(),
        ..Default::default()
    };
    if let Some(written) = written.filter(|value| *value > 0) {
        usage
            .metrics
            .insert("cache_write_tokens".into(), Decimal::from(written));
    }
    Some(usage)
}

pub(super) fn response_value(usage: &NormalizedUsage) -> Value {
    let mut value = serde_json::json!({
        "input_tokens":usage.input_tokens,
        "output_tokens":usage.output_tokens,
        "total_tokens":usage.input_tokens.saturating_add(usage.output_tokens)
    });
    let written = usage
        .metrics
        .get("cache_write_tokens")
        .and_then(|value| value.to_string().parse::<u64>().ok());
    if usage.cached_input_tokens > 0 || written.is_some() {
        let mut details = serde_json::Map::new();
        if usage.cached_input_tokens > 0 {
            details.insert(
                "cached_tokens".into(),
                Value::from(usage.cached_input_tokens),
            );
        }
        if let Some(written) = written {
            details.insert("cache_write_tokens".into(), Value::from(written));
        }
        value["input_tokens_details"] = Value::Object(details);
    }
    value
}

fn number(value: &Value, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        let value = value.get(*name)?;
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}
