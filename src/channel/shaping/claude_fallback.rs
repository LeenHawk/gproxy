//! Opt-in fallback injection for Claude Fable 5.
//!
//! Anthropic-compatible channels use the `server-side-fallback` beta. OpenRouter
//! uses its own Messages `fallbacks` field for multi-model routing.

use http::HeaderMap;
use serde_json::{Value, json};

use super::anthropic_beta;

const FABLE_5: &str = "claude-fable-5";
const OPUS_48: &str = "claude-opus-4-8";
pub const SERVER_SIDE_FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";

/// If the request targets Claude Fable 5, ensure it carries a server-side
/// fallback chain to Opus 4.8 plus the required beta header.
///
/// Existing `fallbacks` are preserved; the beta token is still appended so a
/// user-provided fallback chain works when the channel setting is enabled.
pub fn apply_fable_to_opus48(body: &mut Value, headers: &mut HeaderMap) {
    if apply_fable_to_opus48_body_only(body) {
        anthropic_beta::append_beta_token(headers, SERVER_SIDE_FALLBACK_BETA);
    }
}

/// If the request targets Claude Fable 5, ensure it carries a fallback chain to
/// Opus 4.8 without touching headers.
///
/// Used by OpenRouter, whose Anthropic Messages `fallbacks` field is handled by
/// OpenRouter's multi-model routing rather than Anthropic's beta.
pub fn apply_fable_to_opus48_body_only(body: &mut Value) -> bool {
    let Some(root) = body.as_object_mut() else {
        return false;
    };
    let Some(model) = root.get("model").and_then(Value::as_str) else {
        return false;
    };
    let Some(fallback_model) = fallback_model_for(model) else {
        return false;
    };

    if !root.contains_key("fallbacks") {
        root.insert("fallbacks".into(), json!([{ "model": fallback_model }]));
    }
    true
}

fn fallback_model_for(model: &str) -> Option<String> {
    if model == FABLE_5 {
        return Some(OPUS_48.to_string());
    }

    let (namespace, leaf) = model.rsplit_once('/')?;
    (leaf == FABLE_5).then(|| format!("{namespace}/{OPUS_48}"))
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

        apply_fable_to_opus48(&mut body, &mut headers);

        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
        assert_eq!(header_value(&headers), SERVER_SIDE_FALLBACK_BETA);
    }

    #[test]
    fn preserves_provider_namespace() {
        let mut body = json!({
            "model": "anthropic/claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();

        apply_fable_to_opus48(&mut body, &mut headers);

        assert_eq!(
            body["fallbacks"],
            json!([{ "model": "anthropic/claude-opus-4-8" }])
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

        apply_fable_to_opus48(&mut body, &mut headers);

        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-7" }]));
        assert_eq!(
            header_value(&headers),
            format!("files-api-2025-04-14,{SERVER_SIDE_FALLBACK_BETA}")
        );
    }

    #[test]
    fn ignores_non_fable_models() {
        let mut body = json!({
            "model": "claude-sonnet-4-6",
            "messages": [],
            "max_tokens": 32
        });
        let mut headers = HeaderMap::new();

        apply_fable_to_opus48(&mut body, &mut headers);

        assert!(body.get("fallbacks").is_none());
        assert!(headers.get("anthropic-beta").is_none());
    }

    #[test]
    fn body_only_does_not_touch_headers() {
        let mut body = json!({
            "model": "claude-fable-5",
            "messages": [],
            "max_tokens": 32
        });

        assert!(apply_fable_to_opus48_body_only(&mut body));
        assert_eq!(body["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
    }
}
