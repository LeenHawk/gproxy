//! Same-origin checking for cookie-authenticated requests.
//!
//! **This applies only to callers authenticated by a session cookie.** A
//! cookie is attached by the browser to any request that reaches the origin,
//! including one a foreign page caused; that is the whole of CSRF. An API key
//! is not: it has to be put in a header by whoever is calling, and a foreign
//! page cannot read it. Running this check against API-key callers would
//! break every non-browser client for no gain, so it is a standalone function
//! and the host decides when to call it — after authentication, on a
//! [`CallerKind::Session`](super::CallerKind::Session) caller, before the
//! operation runs.
//!
//! v3 (`crates/gproxy-admin/src/auth/csrf.rs`) passed a request with no
//! `Origin` header. That is not kept: a browser sends `Origin` on every
//! unsafe cross-origin request and on same-origin unsafe requests too, so the
//! absence of one on a `POST` is not a normal browser and treating it as
//! trusted is an opt-out any attacker can take by using a request form that
//! omits the header. Here an unsafe method with no `Origin` is refused.

use crate::AppError;
use http::{HeaderMap, Method, header};

/// Refuse an unsafe request that a foreign origin caused.
///
/// Safe methods (`GET`, `HEAD`, `OPTIONS`) always pass: they are not supposed
/// to change anything, and blocking a cross-origin `GET` is CORS's job, not
/// this one. Every other method requires an `Origin` that either matches the
/// request's own `Host` or is one of `allowed_origins`.
///
/// `allowed_origins` is `AppConfig::cors_origins`: the console served from a
/// separate origin in development, or a portal on another hostname. An empty
/// list means same-origin only.
///
/// Note what this cannot see: `Host` is what the client sent. Behind a proxy
/// that rewrites it, the origin an honest browser reports and the `Host` the
/// instance receives can differ, and the deployment must then name the real
/// origin in `cors_origins`.
pub fn verify_same_origin(
    method: &Method,
    headers: &HeaderMap,
    allowed_origins: &[String],
) -> Result<(), AppError> {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::forbidden("cross-origin request"))?;
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    if matches(origin, host, allowed_origins) {
        Ok(())
    } else {
        Err(AppError::forbidden("cross-origin request"))
    }
}

fn matches(origin: &str, host: Option<&str>, allowed_origins: &[String]) -> bool {
    // `Origin: null` is what a sandboxed iframe, a `data:` document and some
    // redirect chains send. It names no origin, so it can never be this one.
    if origin.eq_ignore_ascii_case("null") {
        return false;
    }
    let Some(authority) = authority_of(origin) else {
        return false;
    };
    if host.is_some_and(|host| host.eq_ignore_ascii_case(authority)) {
        return true;
    }
    allowed_origins.iter().any(|allowed| {
        let allowed = allowed.trim().trim_end_matches('/');
        allowed.eq_ignore_ascii_case(origin.trim_end_matches('/'))
            || authority_of(allowed).is_some_and(|value| value.eq_ignore_ascii_case(authority))
    })
}

/// The `host[:port]` of an origin. An origin is `scheme://host[:port]` with no
/// path, and anything that does not parse that way is not one — which is a
/// refusal, not a fallback to comparing the raw string.
fn authority_of(origin: &str) -> Option<&str> {
    let rest = origin.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    (!authority.is_empty()).then_some(authority)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(origin: Option<&str>, host: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
        }
        if let Some(host) = host {
            headers.insert(header::HOST, host.parse().unwrap());
        }
        headers
    }

    #[test]
    fn a_safe_method_never_needs_an_origin() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(verify_same_origin(&method, &request(None, None), &[]).is_ok());
            assert!(
                verify_same_origin(
                    &method,
                    &request(Some("https://evil.example"), Some("gproxy.local")),
                    &[],
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn an_unsafe_method_without_an_origin_is_refused() {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let error =
                verify_same_origin(&method, &request(None, Some("gproxy.local")), &[]).unwrap_err();
            assert_eq!(error.status_code(), 403);
            assert!(error.to_string().contains("cross-origin request"));
        }
        // An empty header value is an absent one.
        assert!(
            verify_same_origin(&Method::POST, &request(Some(""), Some("gproxy.local")), &[])
                .is_err()
        );
    }

    #[test]
    fn the_requests_own_origin_passes() {
        for (origin, host) in [
            ("https://gproxy.local", "gproxy.local"),
            ("http://gproxy.local:7070", "gproxy.local:7070"),
            ("https://GPROXY.local", "gproxy.local"),
            ("http://127.0.0.1:7070", "127.0.0.1:7070"),
        ] {
            assert!(
                verify_same_origin(&Method::POST, &request(Some(origin), Some(host)), &[]).is_ok(),
                "{origin} vs {host}"
            );
        }
    }

    #[test]
    fn a_foreign_origin_is_refused() {
        for (origin, host) in [
            ("https://evil.example", "gproxy.local"),
            // A different port is a different origin.
            ("http://gproxy.local:7071", "gproxy.local:7070"),
            // And a prefix is not a match.
            ("https://gproxy.local.evil.example", "gproxy.local"),
            ("null", "gproxy.local"),
            ("not an origin", "gproxy.local"),
        ] {
            assert!(
                verify_same_origin(&Method::POST, &request(Some(origin), Some(host)), &[]).is_err(),
                "{origin} vs {host}"
            );
        }
    }

    #[test]
    fn a_configured_origin_passes_against_any_host() {
        let allowed = ["https://console.example.com".to_string()];
        assert!(
            verify_same_origin(
                &Method::POST,
                &request(Some("https://console.example.com"), Some("gproxy.local")),
                &allowed,
            )
            .is_ok()
        );
        // With a trailing slash on either side.
        assert!(
            verify_same_origin(
                &Method::POST,
                &request(Some("https://console.example.com/"), Some("gproxy.local")),
                &["https://console.example.com/".to_string()],
            )
            .is_ok()
        );
        // Still refuses everything not on the list.
        assert!(
            verify_same_origin(
                &Method::POST,
                &request(Some("https://other.example.com"), Some("gproxy.local")),
                &allowed,
            )
            .is_err()
        );
    }

    #[test]
    fn a_request_with_no_host_can_still_name_a_configured_origin() {
        // HTTP/2 carries `:authority`, which a host may not copy into `Host`.
        assert!(
            verify_same_origin(
                &Method::POST,
                &request(Some("https://console.example.com"), None),
                &["https://console.example.com".to_string()],
            )
            .is_ok()
        );
        assert!(
            verify_same_origin(
                &Method::POST,
                &request(Some("https://console.example.com"), None),
                &[],
            )
            .is_err()
        );
    }
}
