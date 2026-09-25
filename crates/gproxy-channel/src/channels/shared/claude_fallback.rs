//! Anthropic server-side fallback configuration shared by API and OAuth channels.

use http::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};

/// What the provider wants Anthropic to do when the requested model is
/// unavailable and the request itself names no `fallbacks` (v3
/// `shared/claude/fallback.rs`).
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackMode {
    /// Never add a `fallbacks` field; the request stands or fails alone.
    #[default]
    Off,
    /// `"fallbacks": "default"` — let Anthropic pick the chain.
    Default,
    /// `"fallbacks": [{"model": ...}]` from `fallback_models`.
    Models,
}

/// The beta an explicit fallback chain needs.
const FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";
/// The beta `"fallbacks": "default"` needs.
const DEFAULT_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Anthropic accepts at most three fallback hops.
const MAX_FALLBACKS: usize = 3;

/// Models that already fall back server-side, so a `fallbacks` field would
/// only narrow what Anthropic would have done anyway.
const FALLBACK_UNSUPPORTED: &[&str] = &[
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
];

/// Carry the namespace of `model` onto `fallback`, so a prefixed model id
/// (a gateway's `vendor/claude-...`) names a sibling rather than a bare id.
fn namespaced(model: &str, fallback: &str) -> String {
    if !fallback.starts_with("claude-") {
        return fallback.to_owned();
    }
    let namespace = model.rfind("claude-").map_or("", |at| &model[..at]);
    format!("{namespace}{fallback}")
}

/// Install the configured fallback chain on a request that names none. A
/// request that already carries `fallbacks` keeps it and only has the matching
/// beta declared.
pub(crate) fn fallbacks(
    body: &mut Value,
    headers: &mut HeaderMap,
    mode: &FallbackMode,
    models: &[String],
) {
    if *mode == FallbackMode::Off
        || body
            .get("fallback_credit_token")
            .is_some_and(|value| !value.is_null())
    {
        return;
    }
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root.get("model").and_then(Value::as_str).map(str::to_owned) else {
        return;
    };
    if let Some(existing) = root.get("fallbacks").filter(|value| !value.is_null()) {
        let beta = if existing == "default" {
            DEFAULT_FALLBACK_BETA
        } else {
            FALLBACK_BETA
        };
        set_fallback_beta(headers, beta);
        return;
    }
    if FALLBACK_UNSUPPORTED
        .iter()
        .any(|unsupported| model.contains(unsupported))
    {
        return;
    }
    let chain = match mode {
        FallbackMode::Models => {
            let mut chain: Vec<Value> = Vec::new();
            for candidate in models {
                let candidate = namespaced(&model, candidate.trim());
                if candidate.is_empty()
                    || candidate == model
                    || chain.iter().any(|entry| entry["model"] == candidate)
                {
                    continue;
                }
                chain.push(json!({"model": candidate}));
                if chain.len() == MAX_FALLBACKS {
                    break;
                }
            }
            if chain.is_empty() {
                None
            } else {
                Some((Value::Array(chain), FALLBACK_BETA))
            }
        }
        FallbackMode::Default | FallbackMode::Off => None,
    }
    // An empty or fully filtered chain falls back to Anthropic's own.
    .unwrap_or((json!("default"), DEFAULT_FALLBACK_BETA));
    root.insert("fallbacks".into(), chain.0);
    set_fallback_beta(headers, chain.1);
}

/// Exactly one server-side fallback beta may be declared; the other is the
/// wrong shape for the `fallbacks` value being sent.
fn set_fallback_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = beta_values(headers);
    values.retain(|value| value != FALLBACK_BETA && value != DEFAULT_FALLBACK_BETA);
    push(&mut values, beta);
    write_beta(headers, values);
}

fn push(values: &mut Vec<String>, beta: &str) {
    if !values.iter().any(|value| value == beta) {
        values.push(beta.to_owned());
    }
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

/// Reseller fallback has no Anthropic model exclusion or beta requirement.
pub(crate) fn reseller(
    body: &mut Value,
    mode: &FallbackMode,
    models: &[String],
    claude: bool,
) -> bool {
    if *mode == FallbackMode::Off
        || body
            .get("fallback_credit_token")
            .is_some_and(|v| !v.is_null())
        || ["fallbacks", "models"]
            .iter()
            .any(|key| body.get(key).is_some_and(|v| !v.is_null()))
    {
        return false;
    }
    let Some(primary) = body.get("model").and_then(Value::as_str) else {
        return false;
    };
    let mut chain = Vec::new();
    if *mode == FallbackMode::Models {
        for model in models {
            let candidate = crate::channel::claude_fallback_model(primary, model.trim());
            if !candidate.is_empty() && candidate != primary && !chain.contains(&candidate) {
                chain.push(candidate);
            }
            if chain.len() == MAX_FALLBACKS {
                break;
            }
        }
    }
    if chain.is_empty() {
        let recommended = crate::channel::claude_fallback_model(primary, "claude-opus-4-8");
        if recommended == primary {
            return false;
        }
        chain.push(recommended);
    }
    if claude {
        body["fallbacks"] = json!(
            chain
                .into_iter()
                .map(|model| json!({"model":model}))
                .collect::<Vec<_>>()
        );
    } else {
        // OpenRouter tries model first, followed by models in this order.
        body["models"] = json!(chain);
    }
    true
}
