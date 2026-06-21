//! Inbound API-key authentication against the control-plane snapshot.

use std::sync::Arc;

use http::HeaderMap;
use http::header::AUTHORIZATION;

use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::pipeline::error::PipelineError;

/// Extract the inbound API token. Accepts the four credential presentations a
/// multi-protocol gateway sees, in priority order:
/// 1. `Authorization: Bearer <tok>` (OpenAI; prefix stripped, ASCII case-insensitive)
/// 2. `x-api-key: <tok>` (Claude)
/// 3. `x-goog-api-key: <tok>` (Gemini header)
/// 4. `?key=<tok>` query parameter (Gemini AI Studio)
///
/// Returns the BARE token; empty values are skipped. (The `?key=` value is taken
/// verbatim — API keys are not percent-encoded in practice.)
pub fn extract_bearer(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        // RFC 9110: the auth-scheme is case-insensitive (`BEARER`, `bearer`, …).
        if v.len() > 7 && v[..7].eq_ignore_ascii_case("bearer ") {
            let rest = v[7..].trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
            // empty Bearer value → fall through
        }
    }
    for header in ["x-api-key", "x-goog-api-key"] {
        if let Some(tok) = headers.get(header).and_then(|v| v.to_str().ok()) {
            let tok = tok.trim();
            if !tok.is_empty() {
                return Some(tok.to_string());
            }
        }
    }
    if let Some(query) = query {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("key=") {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Digest used to index `keys_by_digest`. M1: lowercase hex of `blake3(token)`.
/// SINGLE SOURCE OF TRUTH — both seeding and [`authenticate`] call this on the
/// identical bare token. No salt/pepper: keys are 256-bit CSPRNG, server-issued
/// only, so the entropy already defeats the precomputation/brute-force that
/// salt/pepper exist to stop.
pub fn key_digest(bare_token: &str) -> String {
    blake3::hash(bare_token.as_bytes()).to_hex().to_string()
}

/// Resolve an inbound API key → digest → snapshot identity. No DB hit. 401
/// short-circuits HERE, before any upstream candidate is built.
pub fn authenticate(
    cp: &ControlPlaneSnapshot,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<Arc<KeyIdentity>, PipelineError> {
    let token = extract_bearer(headers, query).ok_or(PipelineError::Unauthorized)?;
    let digest = key_digest(&token);
    cp.keys_by_digest
        .get(&digest)
        .cloned()
        .ok_or(PipelineError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(k.parse::<http::HeaderName>().unwrap(), v.parse().unwrap());
        }
        h
    }

    #[test]
    fn accepts_four_inbound_forms() {
        assert_eq!(
            extract_bearer(&headers(&[("authorization", "Bearer abc")]), None).as_deref(),
            Some("abc")
        );
        // Scheme is case-insensitive (RFC 9110).
        assert_eq!(
            extract_bearer(&headers(&[("authorization", "BEARER abc")]), None).as_deref(),
            Some("abc")
        );
        assert_eq!(
            extract_bearer(&headers(&[("x-api-key", "k1")]), None).as_deref(),
            Some("k1")
        );
        assert_eq!(
            extract_bearer(&headers(&[("x-goog-api-key", "k2")]), None).as_deref(),
            Some("k2")
        );
        assert_eq!(
            extract_bearer(&HeaderMap::new(), Some("alt=1&key=k3&z=2")).as_deref(),
            Some("k3")
        );
        assert_eq!(extract_bearer(&HeaderMap::new(), None), None);
    }

    #[test]
    fn bearer_wins_and_empty_falls_through() {
        let h = headers(&[("authorization", "Bearer top"), ("x-api-key", "k")]);
        assert_eq!(extract_bearer(&h, None).as_deref(), Some("top"));

        let h = headers(&[("authorization", "Bearer "), ("x-api-key", "k")]);
        assert_eq!(extract_bearer(&h, None).as_deref(), Some("k"));
    }
}
