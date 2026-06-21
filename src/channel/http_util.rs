//! Shared HTTP plumbing for channels: absolute-URL join, inbound header/query
//! allow-listing, and upstream request building.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, Uri};

use crate::channel::ChannelError;

/// Hop-by-hop headers (RFC 7230 §6.1) — stripped from the upstream RESPONSE
/// before relaying it to the client (egress), and reused by the pipeline's
/// global inbound blacklist ([`crate::pipeline::ingress`]). Headers nominated
/// by a `Connection:` value are hop-by-hop too — see [`connection_nominated`].
pub(crate) const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
];

/// Lowercased header names nominated as hop-by-hop by `Connection:` values
/// (RFC 7230 §6.1) — they must be stripped alongside the fixed [`HOP_BY_HOP`]
/// set, on both the response egress and the inbound global blacklist.
pub(crate) fn connection_nominated(src: &HeaderMap) -> Vec<String> {
    src.get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Universal inbound headers forwarded upstream by every channel. Channels add
/// their own protocol headers via `Channel::forward_headers` — the allow-list is
/// channel-level, so `openai-*` only ride the OpenAI channel and `anthropic-beta`
/// only Claude, rather than a blind union.
const BASE_FORWARD_HEADERS: &[&str] = &["content-type", "accept"];

/// Allow-list filter for INBOUND headers (client → upstream): keeps the base set
/// plus the channel's `extra`; drops everything else (client auth, cookies,
/// `Host`, hop-by-hop, user-agent, SDK headers). The channel injects the
/// credential's auth itself; the upstream transport derives a fresh `Host` /
/// `:authority` from the request URI (see [`build_request`]).
///
/// `extra` entries MUST be lowercase (compared against the lowercase `HeaderName`).
pub fn allow_headers(src: &HeaderMap, extra: &[&str]) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(src.len());
    for (name, value) in src.iter() {
        let n = name.as_str();
        if BASE_FORWARD_HEADERS.contains(&n) || extra.contains(&n) {
            out.append(name.clone(), value.clone());
        }
    }
    out
}

/// Allow-list filter for INBOUND query parameters: keeps only `key=value` pairs
/// whose key is in the channel's `allowed` set (order preserved); drops the rest,
/// including an inbound `?key=` used solely for downstream auth. `None` if empty.
pub fn allow_query(query: Option<&str>, allowed: &[&str]) -> Option<String> {
    let kept: Vec<&str> = query?
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            !key.is_empty() && allowed.contains(&key)
        })
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join("&"))
    }
}

/// Drop hop-by-hop headers — the fixed set plus any header a `Connection:`
/// value nominates — from an upstream response before relaying to the client
/// (egress). Keeps everything else (content-type, rate-limit headers, etc.) —
/// an allow-list here would discard useful provider headers.
pub fn sanitize_response_headers(src: &HeaderMap) -> HeaderMap {
    let nominated = connection_nominated(src);
    let mut out = HeaderMap::with_capacity(src.len());
    for (name, value) in src.iter() {
        let n = name.as_str();
        if HOP_BY_HOP.contains(&n) || nominated.iter().any(|t| t == n) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}

/// Compose an ABSOLUTE upstream URI from `base_url` + provider-relative `path`
/// (+ optional `query`). Trims one trailing `/` off the base. Errors if the
/// result is not absolute (missing scheme/authority).
pub fn join_url(base_url: &str, path: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(ChannelError::MissingSetting("base_url"));
    }
    let mut url = format!("{base}{path}");
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }
    let uri: Uri = url
        .parse()
        .map_err(|e| ChannelError::Build(format!("bad upstream url {url:?}: {e}")))?;
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(ChannelError::Build(format!(
            "upstream url not absolute: {url:?}"
        )));
    }
    Ok(uri)
}

/// Build the upstream request: method + absolute URI + sanitized headers, with
/// `body` moved in. Channel-specific auth headers are inserted by the caller
/// AFTER this.
///
/// Does NOT set an explicit `Host` header — the transport derives `:authority`
/// (HTTP/2) / `Host` (HTTP/1.1) from the URI. An explicit `Host` header breaks
/// wreq's HTTP/2 send (h2 carries the authority as a pseudo-header and rejects a
/// duplicate `Host`). Any `user:pass@` userinfo the `http` crate kept from
/// `base_url` is stripped from the authority so it never reaches `:authority` /
/// `Host`.
pub fn build_request(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Request<Bytes>, ChannelError> {
    let uri = strip_userinfo(uri)?;
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .body(body)
        .map_err(|e| ChannelError::Build(e.to_string()))?;
    *req.headers_mut() = headers;
    Ok(req)
}

/// Strip any `user:pass@` userinfo from a URI's authority. The `http` crate keeps
/// userinfo from `base_url` in the authority, and it must never leak into
/// `:authority` / `Host`. No-op when the authority has none (the common case).
fn strip_userinfo(uri: Uri) -> Result<Uri, ChannelError> {
    let Some(auth) = uri.authority() else {
        return Ok(uri);
    };
    if !auth.as_str().contains('@') {
        return Ok(uri);
    }
    let clean = match auth.port_u16() {
        Some(port) => format!("{}:{}", auth.host(), port),
        None => auth.host().to_string(),
    };
    let mut parts = uri.into_parts();
    parts.authority = Some(
        clean
            .parse()
            .map_err(|e| ChannelError::Build(format!("authority parse: {e}")))?,
    );
    Uri::from_parts(parts).map_err(|e| ChannelError::Build(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_is_absolute_and_trims_slash() {
        let uri = join_url("http://127.0.0.1:9009/", "/v1/chat/completions", None).unwrap();
        assert_eq!(uri.to_string(), "http://127.0.0.1:9009/v1/chat/completions");
        assert!(uri.scheme().is_some() && uri.authority().is_some());
    }

    #[test]
    fn join_appends_query() {
        let uri = join_url("http://h", "/v1/x", Some("a=1&b=2")).unwrap();
        assert_eq!(uri.to_string(), "http://h/v1/x?a=1&b=2");
    }

    #[test]
    fn missing_base_is_err() {
        assert!(matches!(
            join_url("  ", "/v1/x", None),
            Err(ChannelError::MissingSetting("base_url"))
        ));
    }

    #[test]
    fn build_request_sets_no_explicit_host() {
        // The transport derives :authority (HTTP/2) / Host (HTTP/1.1); an explicit
        // Host header breaks wreq's HTTP/2 send, so build_request must not set one.
        let uri = join_url(
            "https://us-central1-aiplatform.googleapis.com",
            "/v1beta1/x",
            None,
        )
        .unwrap();
        let req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new()).unwrap();
        assert!(req.headers().get(http::header::HOST).is_none());
        assert_eq!(
            req.uri().host(),
            Some("us-central1-aiplatform.googleapis.com")
        );
    }

    #[test]
    fn build_request_strips_userinfo_from_authority() {
        let uri: Uri = "https://user:pass@example.com:8443/x".parse().unwrap();
        let req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new()).unwrap();
        assert!(req.headers().get(http::header::HOST).is_none());
        assert_eq!(
            req.uri().authority().map(|a| a.as_str()),
            Some("example.com:8443")
        );
    }

    #[test]
    fn allow_headers_is_default_deny() {
        let mut h = HeaderMap::new();
        h.insert(
            http::header::AUTHORIZATION,
            "Bearer client".parse().unwrap(),
        );
        h.insert("x-api-key", "client".parse().unwrap());
        h.insert("cookie", "sid=1".parse().unwrap());
        h.insert(http::header::USER_AGENT, "sdk/1".parse().unwrap());
        h.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        h.insert("anthropic-beta", "x".parse().unwrap());

        let out = allow_headers(&h, &["anthropic-beta"]);
        // base allow-listed
        assert_eq!(
            out.get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        // channel extra allow-listed
        assert_eq!(out.get("anthropic-beta").unwrap(), "x");
        // everything else dropped
        assert!(out.get(http::header::AUTHORIZATION).is_none());
        assert!(out.get("x-api-key").is_none());
        assert!(out.get("cookie").is_none());
        assert!(out.get(http::header::USER_AGENT).is_none());
    }

    #[test]
    fn allow_query_keeps_only_listed() {
        // inbound ?key= (downstream auth) is dropped; channel-allowed alt kept
        assert_eq!(
            allow_query(Some("key=secret&alt=sse&x=1"), &["alt"]).as_deref(),
            Some("alt=sse")
        );
        assert_eq!(allow_query(Some("key=secret"), &["alt"]), None);
        assert_eq!(allow_query(None, &["alt"]), None);
    }

    /// Regression: only the fixed HOP_BY_HOP list was stripped — a header
    /// nominated via `Connection: <token>` (RFC 7230 §6.1) leaked through.
    #[test]
    fn sanitize_strips_connection_nominated_headers() {
        let mut h = HeaderMap::new();
        h.insert(
            http::header::CONNECTION,
            "keep-alive, X-Strip-Me".parse().unwrap(),
        );
        h.insert("x-strip-me", "v".parse().unwrap());
        h.insert("x-ratelimit-remaining", "9".parse().unwrap());

        let out = sanitize_response_headers(&h);
        assert!(out.get(http::header::CONNECTION).is_none());
        assert!(out.get("x-strip-me").is_none(), "nominated token kept");
        assert_eq!(out.get("x-ratelimit-remaining").unwrap(), "9");
    }
}
