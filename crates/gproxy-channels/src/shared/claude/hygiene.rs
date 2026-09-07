use gproxy_channel_api::ChannelError;
use http::{HeaderMap, HeaderValue};
use serde_json::Value;

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const THINKING_DISPLAY_UPDATES_BETA: &str = "thinking-display-updates-2026-08-18";

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

pub(crate) fn messages(body: &mut Value, headers: &mut HeaderMap) {
    if gproxy_channel_api::has_fallback_credit(body) {
        return;
    }
    super::cache::sanitize(body);
    strip_sampling(body);
    coerce_prefill(body);
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
    strip_beta(headers, CONTEXT_1M_BETA);
}

pub(crate) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
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

pub(crate) fn coerce_prefill(body: &mut Value) {
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
    let text_prefill = match last.get("content") {
        Some(Value::String(_)) => true,
        Some(Value::Array(blocks)) => {
            !blocks.is_empty()
                && blocks
                    .iter()
                    .all(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        }
        _ => false,
    };
    if !text_prefill {
        return;
    }
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

fn append_thinking_display_beta(body: &Value, headers: &mut HeaderMap) {
    if body.pointer("/thinking/display").and_then(Value::as_str) == Some("updates") {
        append_beta(headers, THINKING_DISPLAY_UPDATES_BETA);
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

pub(crate) fn json_object(body: &[u8]) -> Result<Value, ChannelError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefill_coercion_never_turns_thinking_into_user_content() {
        let mut text = json!({
            "model":"claude-fable-5",
            "messages":[{"role":"assistant","content":"prefix"}]
        });
        coerce_prefill(&mut text);
        assert_eq!(text["messages"][0]["role"], "user");

        let mut thinking = json!({
            "model":"claude-fable-5",
            "messages":[{"role":"assistant","content":[{
                "type":"thinking","thinking":"","signature":"opaque"
            }]}]
        });
        coerce_prefill(&mut thinking);
        assert_eq!(thinking["messages"][0]["role"], "assistant");
    }
}
