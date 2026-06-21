//! Same-origin (CSRF) guard — cross-target (native + edge wasm).
//!
//! Extracted from `http::server::admin::middleware` so the edge dispatcher can
//! reuse the same logic without pulling in axum. The function signature accepts
//! `http::Method` and `http::HeaderMap` directly, matching what both axum
//! (`req.method()`, `req.headers()`) and the edge `http::request::Parts` expose.

use http::Method;
use http::header::{COOKIE, HOST, ORIGIN, REFERER};

/// Same-origin guard for cookie-authenticated mutations — defense-in-depth atop
/// the session cookie's `SameSite=Lax` attribute.
///
/// Enforced ONLY when (a) the method is state-changing and (b) the request
/// carries a session cookie. A browser auto-sends the cookie cross-site but
/// cannot forge/suppress an `Origin` on a non-GET fetch; header-auth (API key)
/// clients send no session cookie, so curl/CI automation is never affected.
///
/// Decision: if neither `Origin` nor `Referer` is present we pass (no browser
/// cross-origin signal exists — `SameSite=Lax` is the backstop, and non-browser
/// callers don't carry the ambient cookie anyway). If either header IS present,
/// its authority must equal the request's own `Host`; a cross-origin or
/// unparseable (`Origin: null`) value is refused. NOTE: behind a reverse proxy
/// the `Host` header must reflect the public origin (`proxy_set_header Host
/// $host`).
pub fn csrf_ok(method: &Method, headers: &http::HeaderMap, allowed_origins: &[String]) -> bool {
    let state_changing = !matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    );
    if !state_changing {
        return true;
    }
    let has_session = headers
        .get(COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(crate::admin::session::parse_cookie)
        .is_some();
    if !has_session {
        return true;
    }
    let origin = headers.get(ORIGIN);
    let referer = headers.get(REFERER);
    if origin.is_none() && referer.is_none() {
        // No cross-origin signal to verify; SameSite=Lax already blocks the
        // cross-site form/navigation case for a non-forgeable cookie.
        return true;
    }
    let host = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase());
    let claimed_url = origin
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned)
        .or_else(|| referer.and_then(|h| h.to_str().ok()).map(str::to_owned));
    let claimed_authority = claimed_url.as_deref().and_then(authority_of);

    // Same-origin: the Host header carries no scheme, so compare authorities.
    if let (Some(h), Some(c)) = (host.as_deref(), claimed_authority.as_deref())
        && h == c
    {
        return true;
    }
    // Cross-origin: pass iff the FULL claimed origin (scheme + authority) exactly
    // matches a configured CORS origin. Scheme-sensitive on purpose — an
    // authority-only compare would let `http://allowed-host` satisfy an
    // `https://allowed-host` allow-list entry (scheme-downgrade CSRF bypass).
    if let Some(claimed_origin) = claimed_url.as_deref().and_then(origin_of)
        && allowed_origins
            .iter()
            .filter_map(|o| origin_of(o))
            .any(|allowed| allowed == claimed_origin)
    {
        return true;
    }
    false
}

/// Extract the lowercased `host[:port]` authority from an absolute URL
/// (`scheme://host[:port]/...`). `Origin: null` and relative values yield
/// `None`.
fn authority_of(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let authority = after_scheme.split('/').next()?.trim();
    (!authority.is_empty()).then(|| authority.to_ascii_lowercase())
}

/// Extract the lowercased `scheme://host[:port]` origin from an absolute URL,
/// stripping any path (a `Referer` carries one). Scheme-sensitive so the CORS
/// allow-list cannot be satisfied by a downgraded scheme. `Origin: null` and
/// relative values yield `None`.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next()?.trim();
    (!scheme.is_empty() && !authority.is_empty()).then(|| {
        format!(
            "{}://{}",
            scheme.to_ascii_lowercase(),
            authority.to_ascii_lowercase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use http::header::{COOKIE, HOST, ORIGIN, REFERER};

    fn headers(cookie: Option<&str>, host: Option<&str>, origin: Option<&str>) -> HeaderMap {
        let mut map = HeaderMap::new();
        if let Some(c) = cookie {
            map.insert(COOKIE, c.parse().unwrap());
        }
        if let Some(h) = host {
            map.insert(HOST, h.parse().unwrap());
        }
        if let Some(o) = origin {
            map.insert(ORIGIN, o.parse().unwrap());
        }
        map
    }

    const SESSION: &str = "gproxy_session=abc123";

    #[test]
    fn get_is_never_blocked() {
        assert!(csrf_ok(
            &Method::GET,
            &headers(Some(SESSION), Some("h"), Some("https://evil")),
            &[]
        ));
    }

    #[test]
    fn header_auth_mutation_without_session_cookie_passes() {
        // No session cookie → API-key/automation path, CSRF N/A.
        assert!(csrf_ok(
            &Method::DELETE,
            &headers(None, Some("gp.example"), None),
            &[]
        ));
        // An unrelated cookie that isn't our session also passes.
        assert!(csrf_ok(
            &Method::POST,
            &headers(
                Some("other=1"),
                Some("gp.example"),
                Some("https://evil.example")
            ),
            &[]
        ));
    }

    #[test]
    fn same_origin_cookie_mutation_passes() {
        assert!(csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("https://gp.example")
            ),
            &[]
        ));
        // Port-qualified origins match the Host when both carry the port.
        let mut h = headers(Some(SESSION), Some("gp.example:8443"), None);
        h.insert(ORIGIN, "https://gp.example:8443/console".parse().unwrap());
        assert!(csrf_ok(&Method::DELETE, &h, &[]));
    }

    #[test]
    fn cross_origin_cookie_mutation_refused() {
        assert!(!csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("https://evil.example")
            ),
            &[]
        ));
    }

    #[test]
    fn cookie_mutation_without_origin_or_referer_passes() {
        // No browser cross-origin signal at all → SameSite=Lax backstop applies.
        assert!(csrf_ok(
            &Method::DELETE,
            &headers(Some(SESSION), Some("gp.example"), None),
            &[]
        ));
    }

    #[test]
    fn cross_origin_referer_refused_when_origin_absent() {
        let mut h = headers(Some(SESSION), Some("gp.example"), None);
        h.insert(REFERER, "https://evil.example/x".parse().unwrap());
        assert!(!csrf_ok(&Method::POST, &h, &[]));
    }

    #[test]
    fn origin_null_refused() {
        assert!(!csrf_ok(
            &Method::POST,
            &headers(Some(SESSION), Some("gp.example"), Some("null")),
            &[]
        ));
    }

    fn allowed_strs() -> Vec<String> {
        vec!["https://console.example.com".to_string()]
    }

    #[test]
    fn cross_origin_in_allow_list_passes() {
        // POST with session cookie from an allowed cross-origin console → OK.
        assert!(csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("https://console.example.com")
            ),
            &allowed_strs()
        ));
    }

    #[test]
    fn cross_origin_not_in_allow_list_refused() {
        assert!(!csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("https://evil.example.com")
            ),
            &allowed_strs()
        ));
    }

    #[test]
    fn cross_origin_scheme_downgrade_refused() {
        // `http://` must NOT satisfy an `https://` allow-list entry (scheme-
        // downgrade CSRF bypass): the full origin, including scheme, is compared.
        assert!(!csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("http://console.example.com")
            ),
            &allowed_strs()
        ));
    }

    #[test]
    fn empty_allow_list_cross_origin_refused() {
        // When allow-list is empty, cross-origin is always refused (default behavior).
        assert!(!csrf_ok(
            &Method::POST,
            &headers(
                Some(SESSION),
                Some("gp.example"),
                Some("https://console.example.com")
            ),
            &[]
        ));
    }

    #[test]
    fn no_cookie_cross_origin_passes_regardless_of_allow_list() {
        // No session cookie → API-key path; CSRF check skipped, allow-list irrelevant.
        assert!(csrf_ok(
            &Method::POST,
            &headers(None, Some("gp.example"), Some("https://evil.example.com")),
            &[]
        ));
    }
}
