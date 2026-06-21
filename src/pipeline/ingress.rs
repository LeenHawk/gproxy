//! Part 1 of inbound header/query handling: the GLOBAL blacklist.
//!
//! Applied ONCE in the pipeline — after auth, before channel selection — so no
//! channel can ever forward the caller's credentials/cookies upstream. The
//! per-channel allow-list (Part 2) runs later, inside `Channel::prepare`
//! (`channel::http_util::allow_headers` / `allow_query`). The two layers take
//! effect at deliberately different pipeline positions.

use http::HeaderMap;

use crate::channel::http_util::{HOP_BY_HOP, connection_nominated};
use crate::pipeline::context::RequestCtx;

/// Inbound headers globally denied upstream regardless of channel. A hard floor
/// the per-channel allow-list cannot override:
/// - hop-by-hop (`HOP_BY_HOP`);
/// - the caller's own credentials + cookies;
/// - `Host` (a fresh one is derived from the upstream URI);
/// - front-proxy / client-network metadata (would leak the client IP and is
///   meaningless upstream);
/// - `accept-encoding` (compression is managed by the transport, which also
///   matches the impersonated client; a forwarded value breaks auto-decompress
///   and content-encoding stripping).
fn is_denied_header(name: &str) -> bool {
    HOP_BY_HOP.contains(&name)
        || matches!(
            name,
            // caller credentials / cookies
            "authorization" | "x-api-key" | "x-goog-api-key" | "api-key" | "cookie"
            // host (re-derived from the upstream URI)
            | "host"
            // front-proxy / client-network metadata
            | "via" | "forwarded" | "x-forwarded-for" | "x-forwarded-host"
            | "x-forwarded-proto" | "x-real-ip"
            // transport-managed compression
            | "accept-encoding"
        )
}

/// Query parameters globally denied upstream — the inbound `?key=` used solely
/// for downstream (client → proxy) authentication.
const DENIED_QUERY: &[&str] = &["key"];

/// Apply the global blacklist to the request in place (Part 1). MUST run after
/// authentication (which reads the credential headers/params) and before the
/// channel's `prepare`. Headers the caller's `Connection:` nominates are
/// hop-by-hop too (RFC 7230 §6.1) and dropped alongside the fixed set.
pub fn apply_global_blacklist(ctx: &mut RequestCtx) {
    let nominated = connection_nominated(&ctx.headers);
    let mut headers = HeaderMap::with_capacity(ctx.headers.len());
    for (name, value) in ctx.headers.iter() {
        let n = name.as_str();
        if !is_denied_header(n) && !nominated.iter().any(|t| t == n) {
            headers.append(name.clone(), value.clone());
        }
    }
    ctx.headers = headers;
    ctx.query = ctx.query.as_deref().and_then(strip_denied_query);
}

fn strip_denied_query(query: &str) -> Option<String> {
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !DENIED_QUERY.contains(&pair.split('=').next().unwrap_or("")))
        .collect();
    (!kept.is_empty()).then(|| kept.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::RoutingMode;
    use bytes::Bytes;
    use http::Method;

    fn ctx(headers: HeaderMap, query: Option<&str>) -> RequestCtx {
        RequestCtx {
            request_id: "t".into(),
            method: Method::POST,
            path: "/v1/chat/completions".into(),
            query: query.map(str::to_string),
            headers,
            body: Bytes::new(),
            mode: RoutingMode::Aggregated,
            identity: None,
            op: None,
            stream: false,
            route_name: None,
            pending_micros: 0,
        }
    }

    #[test]
    fn strips_creds_cookies_hop_by_hop_keeps_rest() {
        let mut h = HeaderMap::new();
        h.insert(http::header::AUTHORIZATION, "Bearer c".parse().unwrap());
        h.insert("x-goog-api-key", "g".parse().unwrap());
        h.insert("cookie", "s=1".parse().unwrap());
        h.insert(http::header::CONNECTION, "keep-alive".parse().unwrap());
        h.insert(http::header::HOST, "client".parse().unwrap());
        h.insert("via", "1.1 Caddy".parse().unwrap());
        h.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        h.insert(http::header::ACCEPT_ENCODING, "gzip, br".parse().unwrap());
        h.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        // an SDK-fingerprint header survives the global floor (an impersonation
        // channel's allow-list may forward it; the floor must not).
        h.insert("x-stainless-lang", "js".parse().unwrap());
        let mut c = ctx(h, Some("key=secret&alt=sse"));
        apply_global_blacklist(&mut c);

        assert!(c.headers.get(http::header::AUTHORIZATION).is_none());
        assert!(c.headers.get("x-goog-api-key").is_none());
        assert!(c.headers.get("cookie").is_none());
        assert!(c.headers.get(http::header::CONNECTION).is_none());
        assert!(c.headers.get(http::header::HOST).is_none());
        assert!(c.headers.get("via").is_none());
        assert!(c.headers.get("x-forwarded-for").is_none());
        assert!(c.headers.get(http::header::ACCEPT_ENCODING).is_none());
        // non-denied headers survive (the channel allow-list decides them later)
        assert_eq!(
            c.headers.get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(c.headers.get("x-stainless-lang").unwrap(), "js");
        // ?key= dropped, other params survive for the channel allow-list
        assert_eq!(c.query.as_deref(), Some("alt=sse"));
    }
}
