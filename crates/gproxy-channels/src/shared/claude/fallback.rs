use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub(crate) fn apply(body: &mut Value, headers: &mut HeaderMap, configured: &Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root.get("model").and_then(Value::as_str).map(str::to_owned) else {
        return;
    };
    if root.contains_key("fallbacks") || unsupported(&model) {
        return;
    }
    let (fallbacks, beta) = if configured.as_str() == Some("default") {
        (json!("default"), "server-side-fallback-2026-07-01")
    } else if let Some(models) = configured.as_array() {
        let chain = models
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|fallback| !fallback.is_empty())
            .map(|fallback| namespaced(&model, fallback))
            .filter(|fallback| fallback != &model)
            .take(3)
            .map(|model| json!({"model":model}))
            .collect::<Vec<_>>();
        if chain.is_empty() {
            return;
        }
        (Value::Array(chain), "server-side-fallback-2026-06-01")
    } else {
        return;
    };
    root.insert("fallbacks".into(), fallbacks);
    append_beta(headers, beta);
}

fn unsupported(model: &str) -> bool {
    [
        "claude-opus-4-8",
        "claude-opus-4-7",
        "claude-opus-4-6",
        "claude-sonnet-4-6",
        "claude-haiku-4-5",
        "claude-opus-4-5",
        "claude-sonnet-4-5",
        "claude-opus-4-1",
        "claude-sonnet-4-0",
        "claude-opus-4-0",
        "claude-3",
    ]
    .iter()
    .any(|unsupported| model.contains(unsupported))
}

fn namespaced(model: &str, fallback: &str) -> String {
    if !fallback.starts_with("claude-") {
        fallback.into()
    } else {
        let namespace = model
            .rfind("claude-")
            .map(|index| &model[..index])
            .unwrap_or_default();
        format!("{namespace}{fallback}")
    }
}

fn append_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !values.contains(&beta) {
        values.push(beta);
    }
    if let Ok(value) = HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}
