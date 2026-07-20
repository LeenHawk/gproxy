//! Shared inbound API-key extraction, digesting, and indexed verification.

use std::collections::HashMap;

use http::HeaderMap;
use http::header::AUTHORIZATION;

/// Extract the inbound API token. Accepts the four credential presentations a
/// multi-protocol gateway sees, in priority order:
/// 1. `Authorization: Bearer <tok>` (OpenAI; prefix stripped, ASCII case-insensitive)
/// 2. `x-api-key: <tok>` (Claude)
/// 3. `x-goog-api-key: <tok>` (Gemini header)
/// 4. `?key=<tok>` query parameter (Gemini AI Studio)
///
/// Returns the bare token; empty values are skipped. The query value is taken
/// verbatim because API keys are not percent-encoded in practice.
pub fn extract_bearer(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if v.len() > 7 && v[..7].eq_ignore_ascii_case("bearer ") {
            let rest = v[7..].trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
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

/// Digest used to index API-key identities: lowercase hex of `blake3(token)`.
pub fn key_digest(bare_token: &str) -> String {
    blake3::hash(bare_token.as_bytes()).to_hex().to_string()
}

/// Verify an inbound API key against a digest-indexed identity map.
pub fn authenticate<'a, T>(
    keys_by_digest: &'a HashMap<String, T>,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Option<&'a T> {
    let token = extract_bearer(headers, query)?;
    keys_by_digest.get(&key_digest(&token))
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

    #[test]
    fn authenticates_digest_indexed_key() {
        let keys = HashMap::from([(key_digest("abc"), 7)]);
        assert_eq!(
            authenticate(&keys, &headers(&[("authorization", "Bearer abc")]), None),
            Some(&7)
        );
        assert_eq!(
            authenticate(&keys, &headers(&[("x-api-key", "bad")]), None),
            None
        );
    }
}
