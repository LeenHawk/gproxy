//! Shared gateway request correlation and completion events.

use http::{HeaderMap, HeaderValue, StatusCode};
use std::borrow::Cow;

/// Generate the opaque per-request id returned to gateway clients.
pub(crate) fn request_id() -> String {
    crate::util::id::ulid()
}

/// Milliseconds elapsed from a wall-clock start, available on native and wasm.
pub(crate) fn elapsed_ms(started_ms: u64) -> u64 {
    crate::util::time::unix_now_ms().saturating_sub(started_ms)
}

/// Attach the gateway correlation id to a response header map.
pub(crate) fn insert_request_id(headers: &mut HeaderMap, request_id: &str) {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        headers.insert("x-gproxy-request-id", value);
    }
}

/// Record the one completion event for a request whose request span is current.
pub(crate) fn complete_current(status: StatusCode, duration_ms: u64, error: Option<&str>) {
    let span = tracing::Span::current();
    span.record("status", status.as_u16());
    span.record("duration_ms", duration_ms);
    let error = error.map(redact_url_query).unwrap_or_default();
    if status.is_server_error() {
        tracing::warn!(
            status = status.as_u16(),
            duration_ms,
            error = %error,
            "request.completed"
        );
    } else {
        tracing::info!(
            status = status.as_u16(),
            duration_ms,
            error = %error,
            "request.completed"
        );
    }
}

/// Give failures that occur before pipeline execution the same request span and
/// completion event shape as normal pipeline requests.
pub(crate) fn complete_early(
    request_id: &str,
    method: &str,
    path: &str,
    status: StatusCode,
    started_ms: u64,
    error: Option<&str>,
) {
    let duration_ms = elapsed_ms(started_ms);
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %method,
        path = %path,
        model = tracing::field::Empty,
        stream = tracing::field::Empty,
        operation = tracing::field::Empty,
        kind = tracing::field::Empty,
        route = tracing::field::Empty,
        provider = tracing::field::Empty,
        status = status.as_u16(),
        duration_ms,
    );
    span.in_scope(|| complete_current(status, duration_ms, error));
}

/// Prevent transport errors from placing credential-bearing URL queries in
/// operational logs while retaining the useful scheme/host/path and cause.
pub(crate) fn redact_url_query(message: &str) -> Cow<'_, str> {
    if !message.contains("http://") && !message.contains("https://") {
        return Cow::Borrowed(message);
    }
    let mut output = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = [rest.find("http://"), rest.find("https://")]
        .into_iter()
        .flatten()
        .min()
    {
        output.push_str(&rest[..start]);
        let url_tail = &rest[start..];
        let end = url_tail
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"'))
            .unwrap_or(url_tail.len());
        let url = &url_tail[..end];
        if let Some(query) = url.find('?') {
            output.push_str(&url[..query]);
            output.push_str("?[redacted]");
        } else {
            output.push_str(url);
        }
        rest = &url_tail[end..];
    }
    output.push_str(rest);
    Cow::Owned(output)
}
