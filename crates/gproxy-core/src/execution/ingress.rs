use http::HeaderMap;

use crate::boundary::RequestCtx;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
];

const DENIED: &[&str] = &[
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "api-key",
    "cookie",
    "host",
    "via",
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
    "accept-encoding",
];

pub(crate) fn strip(ctx: &mut RequestCtx) {
    let nominated = connection_nominated(&ctx.headers);
    let mut headers = HeaderMap::with_capacity(ctx.headers.len());
    for (name, value) in &ctx.headers {
        let name_str = name.as_str();
        if !HOP_BY_HOP.contains(&name_str)
            && !DENIED.contains(&name_str)
            && !nominated.iter().any(|candidate| candidate == name_str)
        {
            headers.append(name.clone(), value.clone());
        }
    }
    ctx.headers = headers;
    ctx.query = ctx.query.as_deref().and_then(strip_key_query);
}

fn connection_nominated(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect()
}

fn strip_key_query(query: &str) -> Option<String> {
    let kept = query
        .split('&')
        .filter(|pair| pair.split('=').next() != Some("key"))
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}
