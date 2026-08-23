use gproxy_channel_api::ChannelError;
use http::{HeaderMap, HeaderValue};
use serde_json::Value;

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";

const SAMPLING_TOLERANT: &[&str] = &[
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
    "claude-sonnet-4-5",
    "claude-opus-4-5",
    "claude-opus-4-1",
    "claude-sonnet-4-0",
    "claude-sonnet-4-20",
    "claude-opus-4-0",
    "claude-opus-4-20",
    "claude-3-opus",
    "claude-3-haiku",
];

const PREFILL_TOLERANT: &[&str] = &[
    "claude-3-opus",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
];

pub(super) fn messages(body: &mut Value, headers: &mut HeaderMap) {
    super::cache::sanitize(body);
    strip_sampling(body);
    coerce_prefill(body);
    append_fast_beta(body, headers);
    strip_beta(headers, CONTEXT_1M_BETA);
}

pub(super) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    append_fast_beta(body, headers);
}

fn strip_sampling(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let tolerant = root
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        != Some("enabled")
        && root
            .get("model")
            .and_then(Value::as_str)
            .is_some_and(|model| {
                SAMPLING_TOLERANT
                    .iter()
                    .any(|prefix| model.starts_with(prefix))
            });
    if tolerant {
        if root.contains_key("temperature") {
            root.remove("top_p");
        }
    } else {
        for name in ["temperature", "top_p", "top_k"] {
            root.remove(name);
        }
    }
}

fn coerce_prefill(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    if !model.contains("claude") || PREFILL_TOLERANT.iter().any(|value| model.contains(value)) {
        return;
    }
    let Some(last) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.last_mut())
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if !matches!(
        last.get("role").and_then(Value::as_str),
        Some("user" | "tool")
    ) {
        last.insert("role".into(), Value::String("user".into()));
    }
}

fn append_fast_beta(body: &Value, headers: &mut HeaderMap) {
    if body.get("speed").and_then(Value::as_str) == Some("fast") {
        append_beta(headers, FAST_MODE_BETA);
    }
}

fn append_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = beta_values(headers);
    if !values.iter().any(|value| value == beta) {
        values.push(beta.into());
    }
    write_beta(headers, values);
}

fn strip_beta(headers: &mut HeaderMap, beta: &str) {
    let values = beta_values(headers)
        .into_iter()
        .filter(|value| value != beta)
        .collect();
    write_beta(headers, values);
}

fn beta_values(headers: &HeaderMap) -> Vec<String> {
    headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn write_beta(headers: &mut HeaderMap, values: Vec<String>) {
    if values.is_empty() {
        headers.remove("anthropic-beta");
    } else if let Ok(value) = HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}

pub(super) fn json_object(body: &[u8]) -> Result<Value, ChannelError> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Prepare(format!("request body is not JSON: {error}")))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(ChannelError::Prepare(
            "request body must be a JSON object".into(),
        ))
    }
}
