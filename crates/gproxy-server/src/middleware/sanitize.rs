use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Headers to strip before forwarding to upstream providers.
const HEADER_DENYLIST: &[&str] = &[
    // Auth credentials — must never leak to upstream
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    // Hop-by-hop / proxy metadata
    "host",
    "via",
    "content-length",
    // Browser context — irrelevant and may fingerprint the client
    "cookie",
    "origin",
    "referer",
    "dnt",
    "priority",
    // Proxy forwarding headers
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    // Cloudflare edge metadata — leaks client geo/IP and proxy topology
    "cdn-loop",
    // Client identity — upstream provider should see its own defaults
    "user-agent",
    "x-title",
    "http-referer",
    "accept",
    "accept-encoding",
    "accept-language",
    "content-type",
    // Anthropic API version — the claudecode/anthropic channels set
    // this authoritatively, and `http::Builder::header` appends rather
    // than replaces, so letting the client's copy through produces a
    // duplicated `anthropic-version` on the wire.
    "anthropic-version",
];

/// Sensitive query parameters to strip.
const AUTH_QUERY_KEYS: &[&str] = &["key"];

fn is_browser_context_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("sec-fetch-") || lower.starts_with("sec-ch-ua")
}

/// Any `cf-*` header is Cloudflare-injected edge metadata (cf-ray,
/// cf-connecting-ip, cf-ipcountry, cf-visitor, cf-ew-via, cf-cert-*, …).
/// None of these should leak to upstream providers.
fn is_cloudflare_header(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("cf-")
}

/// Axum middleware: strip auth headers, browser context headers, and
/// sensitive query params before forwarding to the upstream provider.
pub async fn sanitize_middleware(mut request: Request, next: Next) -> Response {
    let to_remove: Vec<_> = request
        .headers()
        .keys()
        .filter(|name| {
            let s = name.as_str();
            HEADER_DENYLIST
                .iter()
                .any(|denied| s.eq_ignore_ascii_case(denied))
                || is_browser_context_header(s)
                || is_cloudflare_header(s)
        })
        .cloned()
        .collect();
    for name in to_remove {
        request.headers_mut().remove(&name);
    }
    strip_auth_query_params(&mut request);
    next.run(request).await
}

fn strip_auth_query_params(request: &mut Request) {
    let uri = request.uri();
    let Some(query) = uri.query() else {
        return;
    };
    let filtered: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            !AUTH_QUERY_KEYS.contains(&key)
        })
        .collect();
    let new_pq = if filtered.is_empty() {
        uri.path().to_string()
    } else {
        format!("{}?{}", uri.path(), filtered.join("&"))
    };
    if let Ok(new_uri) = new_pq.parse() {
        *request.uri_mut() = new_uri;
    }
}
