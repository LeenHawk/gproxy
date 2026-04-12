use http::HeaderMap;

/// Removes sensitive and browser-specific headers before forwarding upstream.
pub fn sanitize_headers(headers: &mut HeaderMap) {
    let to_remove: Vec<_> = headers
        .keys()
        .filter(|name| {
            let header_name = name.as_str();
            HEADER_DENYLIST
                .iter()
                .any(|denied| header_name.eq_ignore_ascii_case(denied))
                || is_browser_context_header(header_name)
        })
        .cloned()
        .collect();

    for name in to_remove {
        headers.remove(&name);
    }
}

/// Removes sensitive query parameters from a `path?query` string.
pub fn sanitize_query_params(path_and_query: &str) -> String {
    let (path, query) = match path_and_query.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path_and_query, None),
    };
    let Some(query) = query else {
        return path.to_string();
    };

    let filtered_pairs: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or_default();
            !AUTH_QUERY_KEYS.contains(&key)
        })
        .collect();

    if filtered_pairs.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", filtered_pairs.join("&"))
    }
}

const HEADER_DENYLIST: &[&str] = &[
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "host",
    "via",
    "content-length",
    "cookie",
    "origin",
    "referer",
    "dnt",
    "priority",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "user-agent",
    "x-title",
    "http-referer",
    "accept",
    "accept-encoding",
    "accept-language",
    "content-type",
];

const AUTH_QUERY_KEYS: &[&str] = &["key"];

fn is_browser_context_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("sec-fetch-") || lower.starts_with("sec-ch-ua")
}
