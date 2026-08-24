use gproxy_protocol::aws::{CacheTtl, CacheTtlKnown, ServiceTier, StopReason, TokenUsage};
use serde_json::{Value, json};

pub(super) fn claude_usage(tokens: &TokenUsage, tier: Option<&ServiceTier>) -> Value {
    let mut output = serde_json::to_value(tokens).expect("typed usage serializes");
    let root = output.as_object_mut().expect("typed usage is an object");
    rename(root, "inputTokens", "input_tokens");
    rename(root, "outputTokens", "output_tokens");
    rename(root, "cacheReadInputTokens", "cache_read_input_tokens");
    rename(root, "cacheWriteInputTokens", "cache_creation_input_tokens");
    root.remove("totalTokens");
    root.remove("cacheDetails");
    let mut cache_5m = 0_u64;
    let mut cache_1h = 0_u64;
    for detail in tokens.cache_details.iter().flatten() {
        match &detail.ttl {
            CacheTtl::Known(CacheTtlKnown::FiveMinutes) => {
                cache_5m = cache_5m.saturating_add(detail.input_tokens)
            }
            CacheTtl::Known(CacheTtlKnown::OneHour) => {
                cache_1h = cache_1h.saturating_add(detail.input_tokens)
            }
            CacheTtl::Unknown(_) => {}
        }
    }
    if cache_5m > 0 || cache_1h > 0 {
        root.insert(
            "cache_creation".into(),
            json!({
                "ephemeral_5m_input_tokens":cache_5m,
                "ephemeral_1h_input_tokens":cache_1h
            }),
        );
    }
    if let Some(tier) = tier {
        let tier = string(&tier.type_);
        root.insert(
            "service_tier".into(),
            Value::String(if tier == "default" { "standard" } else { &tier }.into()),
        );
        if tier == "priority" {
            root.insert("speed".into(), Value::String("fast".into()));
        }
    }
    output
}

pub(super) fn stop_reason(reason: StopReason) -> Value {
    let reason = string(&reason);
    Value::String(
        match reason.as_str() {
            "guardrail_intervened" | "content_filtered" => "refusal",
            "malformed_model_output" | "malformed_tool_use" => "end_turn",
            other => other,
        }
        .into(),
    )
}

fn string(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .expect("typed enum serializes")
        .as_str()
        .expect("typed string enum serializes as a string")
        .into()
}

fn rename(root: &mut serde_json::Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = root.remove(from) {
        root.insert(to.into(), value);
    }
}
