use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub(crate) fn enabled(settings: &Value) -> bool {
    configured(settings).is_some()
}

pub(crate) fn apply(body: &mut Value, headers: &mut HeaderMap, settings: &Value) {
    let Some(configured) = configured(settings) else {
        return;
    };
    if let Some(beta) = insert(body, &configured, true) {
        append_beta(headers, beta);
    }
}

pub(crate) fn apply_without_beta(body: &mut Value, settings: &Value) {
    if let Some(configured) = configured(settings) {
        insert(body, &configured, false);
    }
}

fn configured(settings: &Value) -> Option<Value> {
    match settings.get("claude_fallback_mode").and_then(Value::as_str) {
        Some("default") => Some(json!("default")),
        Some("models") => settings.get("claude_fallback_models").cloned(),
        Some("off") => None,
        _ => settings.get("claude_fable_fallbacks").cloned(),
    }
}

fn insert(body: &mut Value, configured: &Value, anthropic_policy: bool) -> Option<&'static str> {
    let root = body.as_object_mut()?;
    let model = root.get("model").and_then(Value::as_str)?.to_owned();
    if root.contains_key("fallbacks") || (anthropic_policy && unsupported(&model)) {
        return None;
    }
    let (fallbacks, beta) = if configured.as_str() == Some("default") {
        if anthropic_policy {
            (json!("default"), "server-side-fallback-2026-07-01")
        } else {
            let fallback = namespaced(&model, "claude-opus-4-8");
            if fallback == model {
                return None;
            }
            (
                json!([{"model":fallback}]),
                "server-side-fallback-2026-06-01",
            )
        }
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
            return None;
        }
        (Value::Array(chain), "server-side-fallback-2026-06-01")
    } else {
        return None;
    };
    root.insert("fallbacks".into(), fallbacks);
    Some(beta)
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
