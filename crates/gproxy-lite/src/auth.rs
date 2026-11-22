//! The one access key. A request carries it or is refused.

use http::HeaderMap;
use sha2::{Digest, Sha256};

/// The key a request presents, wherever its dialect puts one: a bearer token,
/// Claude's `x-api-key`, Gemini's `x-goog-api-key` or `?key=`. A header wins
/// over the query.
pub(crate) fn presented(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    let header = |name: &str| headers.get(name)?.to_str().ok().map(str::trim);
    if let Some(value) = header("authorization") {
        let (scheme, token) = value.split_once(' ')?;
        return scheme
            .eq_ignore_ascii_case("bearer")
            .then(|| token.trim().to_owned());
    }
    if let Some(key) = header("x-api-key").or_else(|| header("x-goog-api-key")) {
        return Some(key.to_owned());
    }
    query?.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == "key").then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|b| char::from(*b).to_digit(16));
        match (bytes[index], hex(index + 1), hex(index + 2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            (b'+', ..) => {
                out.push(b' ');
                index += 1;
            }
            (byte, ..) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Constant-time equality. Both sides are hashed first, so neither the length
/// nor a matching prefix of the configured key shows in the timing.
pub(crate) fn matches(presented: &str, configured: &str) -> bool {
    let presented = Sha256::digest(presented.as_bytes());
    let configured = Sha256::digest(configured.as_bytes());
    presented
        .iter()
        .zip(configured.iter())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    http::HeaderName::from_static(name),
                    http::HeaderValue::from_static(value),
                )
            })
            .collect()
    }

    #[test]
    fn every_dialect_presents_the_key_its_own_way() {
        assert_eq!(
            presented(&headers(&[("authorization", "Bearer k1")]), None).as_deref(),
            Some("k1")
        );
        assert_eq!(
            presented(&headers(&[("x-api-key", "k2")]), None).as_deref(),
            Some("k2")
        );
        assert_eq!(
            presented(&headers(&[("x-goog-api-key", "k3")]), None).as_deref(),
            Some("k3")
        );
        assert_eq!(
            presented(&HeaderMap::new(), Some("alt=sse&key=k%2B4")).as_deref(),
            Some("k+4")
        );
        // A header wins, and a scheme other than Bearer presents nothing.
        assert_eq!(
            presented(&headers(&[("x-api-key", "k2")]), Some("key=other")).as_deref(),
            Some("k2")
        );
        assert_eq!(
            presented(&headers(&[("authorization", "Basic abc")]), Some("key=k")),
            None
        );
        assert_eq!(presented(&HeaderMap::new(), Some("alt=sse")), None);
    }

    #[test]
    fn only_the_configured_key_matches() {
        assert!(matches("secret", "secret"));
        assert!(!matches("secret ", "secret"));
        assert!(!matches("", "secret"));
        assert!(!matches("secre", "secret"));
    }
}
