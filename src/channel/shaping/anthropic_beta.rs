//! `anthropic-beta` header token hygiene.

use http::{HeaderMap, HeaderValue};
use serde_json::Value;

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";

/// Add the Fast Mode beta when a native Claude Messages body explicitly asks
/// for `speed: "fast"`. Callers decide whether their upstream is Anthropic
/// direct; this helper deliberately does not infer support from channel type.
pub fn append_fast_mode_beta(headers: &mut HeaderMap, body: &Value) {
    if body.get("speed").and_then(Value::as_str) == Some("fast") {
        append_beta_token(headers, FAST_MODE_BETA);
    }
}

/// Inspect a serialized Claude request without rewriting its body.
pub fn append_fast_mode_beta_from_body(headers: &mut HeaderMap, body: &[u8]) {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        append_fast_mode_beta(headers, &value);
    }
}

/// Append `token` to the `anthropic-beta` header, comma-joining with existing
/// tokens and de-duplicating exact token matches.
pub fn append_beta_token(headers: &mut HeaderMap, token: &str) {
    let existing = headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut tokens: Vec<String> = existing
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.iter().any(|t| t == token) {
        return;
    }
    tokens.push(token.to_string());
    let combined = tokens.join(",");
    if let Ok(value) = HeaderValue::from_str(&combined) {
        headers.insert("anthropic-beta", value);
    }
}

/// Remove every token in `tokens` from the `anthropic-beta` header. The
/// header is rewritten without the stripped tokens; if no tokens remain,
/// the header is removed entirely.
///
/// Used to drop default-on betas that are known to break upstream — e.g.
/// `context-1m-2025-08-07`, which Anthropic currently rejects on the
/// claude-code OAuth path.
///
/// Infallible: if the header is absent (or unreadable as UTF-8), this is a
/// no-op. The rejoined value is built from tokens already present in a valid
/// header, so reconstruction does not fail in practice; on the off chance it
/// does the existing header is left untouched.
pub fn strip_beta_tokens(headers: &mut HeaderMap, tokens: &[&str]) {
    let Some(existing) = headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
    else {
        return;
    };
    let kept: Vec<String> = existing
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && !tokens.iter().any(|drop| t == drop))
        .collect();
    if kept.is_empty() {
        headers.remove("anthropic-beta");
        return;
    }
    let combined = kept.join(",");
    if let Ok(value) = HeaderValue::from_str(&combined) {
        headers.insert("anthropic-beta", value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_value(headers: &HeaderMap) -> String {
        headers
            .get("anthropic-beta")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn strip_removes_target_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("oauth-2025-04-20,context-1m-2025-08-07,files-api-2025-04-14"),
        );
        strip_beta_tokens(&mut headers, &["context-1m-2025-08-07"]);
        assert_eq!(
            header_value(&headers),
            "oauth-2025-04-20,files-api-2025-04-14"
        );
    }

    #[test]
    fn strip_removes_header_when_no_tokens_remain() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("context-1m-2025-08-07"),
        );
        strip_beta_tokens(&mut headers, &["context-1m-2025-08-07"]);
        assert!(headers.get("anthropic-beta").is_none());
    }

    #[test]
    fn strip_is_noop_when_header_absent() {
        let mut headers = HeaderMap::new();
        strip_beta_tokens(&mut headers, &["context-1m-2025-08-07"]);
        assert!(headers.get("anthropic-beta").is_none());
    }

    #[test]
    fn append_inserts_missing_header() {
        let mut headers = HeaderMap::new();
        append_beta_token(&mut headers, "server-side-fallback-2026-06-01");
        assert_eq!(header_value(&headers), "server-side-fallback-2026-06-01");
    }

    #[test]
    fn append_dedupes_existing_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("files-api-2025-04-14"),
        );
        append_beta_token(&mut headers, "files-api-2025-04-14");
        assert_eq!(header_value(&headers), "files-api-2025-04-14");
    }

    #[test]
    fn append_preserves_existing_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("files-api-2025-04-14"),
        );
        append_beta_token(&mut headers, "server-side-fallback-2026-06-01");
        assert_eq!(
            header_value(&headers),
            "files-api-2025-04-14,server-side-fallback-2026-06-01"
        );
    }
}
