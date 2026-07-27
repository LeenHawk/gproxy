//! Opt-in fallback injection for Claude Messages requests.
//!
//! Anthropic-compatible channels use the `server-side-fallback` beta. OpenRouter
//! uses its own Messages `fallbacks` field for multi-model routing.

use http::HeaderMap;
use serde_json::{Map, Value, json};

use super::anthropic_beta;
use crate::channel::settings::ClaudeFableFallbacks;

const OPUS_48: &str = "claude-opus-4-8";
pub const SERVER_SIDE_FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";
pub const DEFAULT_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Ensure the request carries the configured server-side fallback routing plus
/// the matching beta header.
///
/// Existing `fallbacks` are preserved; the beta token is still appended so a
/// user-provided fallback chain works when the channel setting is enabled.
pub fn apply_claude_fallback(
    body: &mut Value,
    headers: &mut HeaderMap,
    configured: &ClaudeFableFallbacks,
) {
    if let Some(beta) = apply(body, configured, true) {
        anthropic_beta::strip_beta_tokens(
            headers,
            &[SERVER_SIDE_FALLBACK_BETA, DEFAULT_FALLBACK_BETA],
        );
        anthropic_beta::append_beta_token(headers, beta);
    }
}

/// Ensure the request carries an OpenRouter fallback chain without touching
/// headers.
///
/// Used by OpenRouter, whose Anthropic Messages `fallbacks` field is handled by
/// OpenRouter's multi-model routing rather than Anthropic's beta.
pub fn apply_openrouter_fallback(body: &mut Value, configured: &ClaudeFableFallbacks) -> bool {
    apply(body, configured, false).is_some()
}

fn apply(
    body: &mut Value,
    configured: &ClaudeFableFallbacks,
    supports_default: bool,
) -> Option<&'static str> {
    let root = body.as_object_mut()?;
    let model = root.get("model").and_then(Value::as_str)?;

    if !root.contains_key("fallbacks") {
        let fallbacks = configured_fallbacks(model, configured, supports_default)?;
        root.insert("fallbacks".into(), fallbacks);
    }
    Some(beta_for(root))
}

fn configured_fallbacks(
    model: &str,
    configured: &ClaudeFableFallbacks,
    supports_default: bool,
) -> Option<Value> {
    match configured {
        ClaudeFableFallbacks::Default(_) => default_fallbacks(model, supports_default),
        ClaudeFableFallbacks::Models(models) => {
            let mut chain = Vec::new();
            for fallback in models
                .iter()
                .map(|model| model.trim())
                .filter(|model| !model.is_empty())
            {
                let fallback = fallback_model_for(model, fallback);
                if fallback != model
                    && !chain.iter().any(|entry: &Value| entry["model"] == fallback)
                {
                    chain.push(json!({ "model": fallback }));
                }
                if chain.len() == 3 {
                    break;
                }
            }
            if !chain.is_empty() {
                return Some(Value::Array(chain));
            }
            default_fallbacks(model, supports_default)
        }
    }
}

fn default_fallbacks(model: &str, supports_default: bool) -> Option<Value> {
    if supports_default {
        return Some(json!("default"));
    }
    opus48_fallbacks(model)
}

fn opus48_fallbacks(model: &str) -> Option<Value> {
    let fallback = fallback_model_for(model, OPUS_48);
    (fallback != model).then(|| json!([{ "model": fallback }]))
}

fn beta_for(root: &Map<String, Value>) -> &'static str {
    if root.get("fallbacks").and_then(Value::as_str) == Some("default") {
        DEFAULT_FALLBACK_BETA
    } else {
        SERVER_SIDE_FALLBACK_BETA
    }
}

fn fallback_model_for(model: &str, fallback: &str) -> String {
    if !fallback.starts_with("claude-") {
        fallback.to_owned()
    } else {
        let namespace = model
            .rfind("claude-")
            .map(|index| &model[..index])
            .unwrap_or_default();
        format!("{namespace}{fallback}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    fn header_value(headers: &HeaderMap) -> String {
        headers
            .get("anthropic-beta")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn injects_fable_to_opus_fallback() {
        let mut body = json!({
            "model": "claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();

        let configured =
            ClaudeFableFallbacks::Default(crate::channel::settings::ClaudeFallbackDefault::Default);
        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(body["fallbacks"], json!("default"));
        assert_eq!(header_value(&headers), DEFAULT_FALLBACK_BETA);
    }

    #[test]
    fn preserves_provider_namespace() {
        let mut body = json!({
            "model": "anthropic/claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();

        let configured = ClaudeFableFallbacks::Models(vec![OPUS_48.to_owned()]);
        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(
            body["fallbacks"],
            json!([{ "model": "anthropic/claude-opus-4-8" }])
        );
    }

    #[test]
    fn preserves_bedrock_namespace() {
        let mut body = json!({
            "model": "anthropic.claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });

        let configured = ClaudeFableFallbacks::Models(vec![OPUS_48.to_owned()]);
        assert!(apply_openrouter_fallback(&mut body, &configured));
        assert_eq!(
            body["fallbacks"],
            json!([{ "model": "anthropic.claude-opus-4-8" }])
        );
    }

    #[test]
    fn preserves_existing_fallbacks_and_appends_beta() {
        let mut body = json!({
            "model": "claude-fable-5",
            "fallbacks": [{ "model": "claude-opus-4-7" }],
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("files-api-2025-04-14"),
        );

        let configured = ClaudeFableFallbacks::Models(vec![OPUS_48.to_owned()]);
        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-7" }]));
        assert_eq!(
            header_value(&headers),
            format!("files-api-2025-04-14,{SERVER_SIDE_FALLBACK_BETA}")
        );
    }

    #[test]
    fn replaces_conflicting_fallback_beta_revision() {
        let mut body = json!({
            "model": "claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });
        let configured =
            ClaudeFableFallbacks::Default(crate::channel::settings::ClaudeFallbackDefault::Default);
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("files-api-2025-04-14,server-side-fallback-2026-06-01"),
        );

        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(
            header_value(&headers),
            "files-api-2025-04-14,server-side-fallback-2026-07-01"
        );
    }

    #[test]
    fn applies_to_non_fable_models() {
        let mut body = json!({
            "model": "claude-sonnet-4-6",
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();

        let configured = ClaudeFableFallbacks::Models(vec![OPUS_48.to_owned()]);
        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
        assert_eq!(header_value(&headers), SERVER_SIDE_FALLBACK_BETA);
    }

    #[test]
    fn body_only_does_not_touch_headers() {
        let mut body = json!({
            "model": "claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });

        let configured = ClaudeFableFallbacks::Models(vec![OPUS_48.to_owned()]);
        assert!(apply_openrouter_fallback(&mut body, &configured));
        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
    }

    #[test]
    fn injects_ordered_custom_chain_with_at_most_three_distinct_models() {
        let mut body = json!({
            "model": "anthropic/claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });
        let configured = ClaudeFableFallbacks::Models(vec![
            "claude-opus-5".into(),
            "claude-opus-4-8".into(),
            "claude-opus-5".into(),
            "claude-sonnet-5".into(),
            "claude-opus-4-7".into(),
        ]);
        let mut headers = HeaderMap::new();

        apply_claude_fallback(&mut body, &mut headers, &configured);

        assert_eq!(
            body["fallbacks"],
            json!([
                {"model": "anthropic/claude-opus-5"},
                {"model": "anthropic/claude-opus-4-8"},
                {"model": "anthropic/claude-sonnet-5"}
            ])
        );
        assert_eq!(header_value(&headers), SERVER_SIDE_FALLBACK_BETA);
    }

    #[test]
    fn openrouter_default_does_not_fallback_to_the_same_model() {
        let mut body = json!({
            "model": "anthropic/claude-opus-4-8",
            "messages": [],
            "max_tokens": 32
        });
        let configured =
            ClaudeFableFallbacks::Default(crate::channel::settings::ClaudeFallbackDefault::Default);

        assert!(!apply_openrouter_fallback(&mut body, &configured));
        assert!(body.get("fallbacks").is_none());
    }
}
