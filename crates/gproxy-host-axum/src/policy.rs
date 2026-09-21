//! Two transport policies that belong to the host and to nothing below it:
//! which browser origins may call this instance, and which address is the
//! client's.
//!
//! # The trusted-proxy rule
//!
//! `x-forwarded-for` and `x-forwarded-proto` are headers, and a header is
//! whatever the peer wrote. They are believed **only when the socket's own
//! peer address is listed in
//! [`AppConfig::trusted_proxies`](gproxy_app::AppConfig)**, or is loopback —
//! a process on the same machine as the gateway has already won. From any
//! other peer they are ignored outright, not merged, not preferred, not used
//! as a fallback.
//!
//! The stakes are different for the two headers, and both matter:
//!
//! - a forged `x-forwarded-for` picks the address that lands in the operator's
//!   log next to somebody's request;
//! - a forged `x-forwarded-proto` picks the **scheme of the OAuth issuer
//!   identifier**, which is a discovery document telling a client where to
//!   send its authorization code.
//!
//! Neither is worth a convenience default, so the default configuration
//! trusts nothing.

use axum::response::Response;
use http::{HeaderMap, HeaderValue, Method, header};

/// The client address, as far as this host can honestly establish it.
///
/// The peer address when the peer is not trusted; the first
/// `x-forwarded-for` hop (or `x-real-ip`) when it is.
pub fn client_ip(
    peer: std::net::IpAddr,
    headers: &HeaderMap,
    trusted: &[String],
) -> std::net::IpAddr {
    if !trusts(peer, trusted) {
        return peer;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or(peer)
}

/// The scheme the client used, for building absolute URLs back to this
/// instance.
///
/// `x-forwarded-proto` is read **only** from a trusted peer. Everywhere else
/// this host answers over plain HTTP itself, so `http` is the truthful answer
/// and a deployment that terminates TLS elsewhere says so by listing its
/// proxy.
pub fn client_scheme(
    peer: std::net::IpAddr,
    headers: &HeaderMap,
    trusted: &[String],
) -> &'static str {
    if !trusts(peer, trusted) {
        return "http";
    }
    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .unwrap_or_default();
    if forwarded.eq_ignore_ascii_case("https") {
        "https"
    } else {
        "http"
    }
}

/// Whether this peer's forwarding headers are believed.
///
/// Loopback is trusted implicitly: a proxy on the same host is the standard
/// deployment, and a process that can already bind loopback next to the
/// gateway can reach the gateway's database anyway.
fn trusts(peer: std::net::IpAddr, trusted: &[String]) -> bool {
    peer.is_loopback() || trusted.iter().any(|entry| entry.trim() == peer.to_string())
}

/// The `Origin` this request carries, if it is one the instance allows.
///
/// An origin that is not on the list answers `None`, which means no CORS
/// headers at all — the browser then refuses the response, which is the
/// correct outcome and not this host's to soften.
pub fn allowed_origin(headers: &HeaderMap, allowed: &[String]) -> Option<HeaderValue> {
    let origin = headers.get(header::ORIGIN)?;
    let text = origin.to_str().ok()?;
    allowed
        .iter()
        .any(|entry| entry.trim().trim_end_matches('/') == text.trim_end_matches('/'))
        .then(|| origin.clone())
}

/// Whether this is a CORS preflight rather than a request to forward.
pub fn is_preflight(method: &Method, headers: &HeaderMap) -> bool {
    method == Method::OPTIONS && headers.contains_key("access-control-request-method")
}

/// The CORS headers of an allowed origin, added to a response.
///
/// `Vary: Origin` is not optional: the same URL answers differently per
/// origin, and a cache that missed that would serve one site's permission to
/// another.
pub fn apply_cors(mut response: Response, origin: Option<&HeaderValue>) -> Response {
    let Some(origin) = origin else {
        return response;
    };
    let headers = response.headers_mut();
    headers.insert("access-control-allow-origin", origin.clone());
    headers.insert(
        "access-control-allow-credentials",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
    );
    headers.insert(
        "access-control-allow-headers",
        HeaderValue::from_static("authorization, content-type, x-api-key, x-goog-api-key"),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    response
}

/// The preflight answer: the CORS headers, plus whichever of the requested
/// headers this host is willing to echo.
///
/// Vendor SDKs send a long tail of telemetry headers (`x-stainless-*`,
/// `anthropic-*`), and a fixed list would have to grow with every SDK release.
/// The namespaces below are echoed when asked for; anything else is not,
/// because echoing an arbitrary request header is how a preflight becomes a
/// permission the caller wrote itself.
pub fn apply_preflight_cors(
    response: Response,
    origin: Option<&HeaderValue>,
    request_headers: &HeaderMap,
) -> Response {
    let mut response = apply_cors(response, origin);
    if origin.is_none() {
        return response;
    }
    const ECHOED_PREFIXES: [&str; 3] = ["x-stainless-", "anthropic-", "x-gproxy-"];
    let mut allowed = vec![
        "authorization".to_owned(),
        "content-type".to_owned(),
        "x-api-key".to_owned(),
        "x-goog-api-key".to_owned(),
    ];
    for value in request_headers.get_all("access-control-request-headers") {
        for name in value.to_str().unwrap_or_default().split(',') {
            // Parsed rather than trimmed: an unparsable name must not reach a
            // response header, and this is also what rejects a value carrying
            // a space or a control character.
            let Ok(name) = name.trim().parse::<http::HeaderName>() else {
                continue;
            };
            let name = name.as_str();
            if ECHOED_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
                && !allowed.iter().any(|item| item == name)
            {
                allowed.push(name.to_owned());
            }
        }
    }
    if let Ok(value) = HeaderValue::from_str(&allowed.join(", ")) {
        response
            .headers_mut()
            .insert("access-control-allow-headers", value);
    }
    response.headers_mut().append(
        header::VARY,
        HeaderValue::from_static("Access-Control-Request-Headers"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn an_untrusted_peers_forwarding_headers_are_ignored() {
        let claimed = headers(&[
            ("x-forwarded-for", "203.0.113.9"),
            ("x-forwarded-proto", "https"),
        ]);
        let untrusted = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

        assert_eq!(client_ip(untrusted, &claimed, &[]), untrusted);
        assert_eq!(client_scheme(untrusted, &claimed, &[]), "http");

        let trusted = [untrusted.to_string()];
        assert_eq!(
            client_ip(untrusted, &claimed, &trusted),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(client_scheme(untrusted, &claimed, &trusted), "https");
    }

    #[test]
    fn loopback_is_trusted_because_a_local_proxy_is_the_normal_deployment() {
        let claimed = headers(&[("x-real-ip", "203.0.113.9")]);
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(
            client_ip(loopback, &claimed, &[]),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn a_trusted_peer_with_no_forwarding_headers_is_still_the_client() {
        let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(
            client_ip(peer, &HeaderMap::new(), &[peer.to_string()]),
            peer
        );
    }

    #[test]
    fn only_a_configured_origin_gets_cors_headers() {
        let allowed = ["https://console.example.com".to_owned()];
        let request = headers(&[("origin", "https://console.example.com")]);
        let origin = allowed_origin(&request, &allowed).unwrap();
        let response = apply_cors(Response::default(), Some(&origin));
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "https://console.example.com"
        );
        assert_eq!(response.headers()[header::VARY], "Origin");

        let foreign = headers(&[("origin", "https://evil.example")]);
        assert!(allowed_origin(&foreign, &allowed).is_none());
        let response = apply_cors(Response::default(), None);
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }

    #[test]
    fn a_preflight_echoes_the_sdk_namespaces_and_nothing_else() {
        let request = headers(&[
            (
                "access-control-request-headers",
                "X-Stainless-Runtime, x-custom-secret, anthropic-version",
            ),
            ("access-control-request-headers", "x-gproxy-session-id"),
        ]);
        let origin = HeaderValue::from_static("https://console.example.com");
        let response = apply_preflight_cors(Response::default(), Some(&origin), &request);
        let echoed = response.headers()["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(echoed.contains("x-stainless-runtime"), "{echoed}");
        assert!(echoed.contains("anthropic-version"), "{echoed}");
        assert!(echoed.contains("x-gproxy-session-id"), "{echoed}");
        assert!(!echoed.contains("x-custom-secret"), "{echoed}");
    }

    #[test]
    fn a_preflight_from_a_foreign_origin_is_told_nothing() {
        let request = headers(&[("access-control-request-headers", "x-stainless-runtime")]);
        let response = apply_preflight_cors(Response::default(), None, &request);
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-headers")
        );
    }

    #[test]
    fn a_preflight_is_an_options_with_a_requested_method() {
        let request = headers(&[("access-control-request-method", "POST")]);
        assert!(is_preflight(&Method::OPTIONS, &request));
        assert!(!is_preflight(&Method::POST, &request));
        assert!(!is_preflight(&Method::OPTIONS, &HeaderMap::new()));
    }
}
