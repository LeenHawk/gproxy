use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub(crate) const RECOMMENDED_MODEL: &str = "claude-opus-4-8";

const FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";
const DEFAULT_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

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
    gproxy_channel_api::claude_fallback_setting(settings)
}

fn insert(body: &mut Value, configured: &Value, anthropic_policy: bool) -> Option<&'static str> {
    let root = body.as_object_mut()?;
    if root
        .get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
    {
        return None;
    }
    let model = root.get("model").and_then(Value::as_str)?.to_owned();
    if anthropic_policy && unsupported(&model) {
        return None;
    }
    if root.get("fallbacks").is_some_and(|value| !value.is_null()) {
        return Some(beta_for(root));
    }
    let (fallbacks, beta) = if configured.as_str() == Some("default") {
        default_chain(&model, anthropic_policy)?
    } else {
        let models = configured.as_array()?;
        let mut chain = models
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|fallback| !fallback.is_empty())
            .map(|fallback| namespaced(&model, fallback))
            .filter(|fallback| fallback != &model)
            .fold(Vec::new(), |mut chain, model| {
                if !chain.iter().any(|entry: &Value| entry["model"] == model) {
                    chain.push(json!({"model":model}));
                }
                chain
            });
        chain.truncate(3);
        if chain.is_empty() {
            default_chain(&model, anthropic_policy)?
        } else {
            (Value::Array(chain), FALLBACK_BETA)
        }
    };
    root.insert("fallbacks".into(), fallbacks);
    Some(beta)
}

/// What "default" means, and where an explicit chain lands when nothing in it is
/// usable: Anthropic's own policy on their surfaces, one hop to Opus elsewhere.
fn default_chain(model: &str, anthropic_policy: bool) -> Option<(Value, &'static str)> {
    if anthropic_policy {
        return Some((json!("default"), DEFAULT_FALLBACK_BETA));
    }
    let fallback = namespaced(model, RECOMMENDED_MODEL);
    (fallback != model).then(|| (json!([{"model":fallback}]), FALLBACK_BETA))
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
        "claude-sonnet-4-20",
        "claude-opus-4-0",
        "claude-opus-4-20",
        "claude-3",
    ]
    .iter()
    .any(|unsupported| model.contains(unsupported))
}

pub(crate) fn namespaced(model: &str, fallback: &str) -> String {
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
        .filter(|value| {
            !value.is_empty() && !matches!(*value, FALLBACK_BETA | DEFAULT_FALLBACK_BETA)
        })
        .collect::<Vec<_>>();
    if !values.contains(&beta) {
        values.push(beta);
    }
    if let Ok(value) = HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}

fn beta_for(root: &serde_json::Map<String, Value>) -> &'static str {
    if root.get("fallbacks").and_then(Value::as_str) == Some("default") {
        DEFAULT_FALLBACK_BETA
    } else {
        FALLBACK_BETA
    }
}
