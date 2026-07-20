//! Target-independent gateway response metadata preparation.

use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::channel::http_util::{HOP_BY_HOP, connection_nominated};
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};

pub(crate) struct ResponseMetadata {
    pub status: StatusCode,
    pub headers: HeaderMap,
}

/// Prepare response status and headers before the native/edge body bridges
/// attach their target-specific buffered or streaming body.
pub(crate) fn metadata(outcome: &ExecOutcome, request_id: &str) -> ResponseMetadata {
    let mut headers = sanitize_headers(&outcome.headers);
    if outcome.status.is_success()
        && matches!(outcome.body, ResponseBody::Stream(_))
        && !headers.contains_key(header::CONTENT_TYPE)
    {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
    }
    if let Ok(value) = HeaderValue::from_str(request_id) {
        headers.insert("x-gproxy-request-id", value);
    }

    ResponseMetadata {
        status: outcome.status,
        headers,
    }
}

/// Drop the fixed and `Connection`-nominated hop-by-hop headers while keeping
/// all end-to-end values, including repeated headers.
fn sanitize_headers(src: &HeaderMap) -> HeaderMap {
    let nominated = connection_nominated(src);
    let mut out = HeaderMap::with_capacity(src.len());
    for (name, value) in src {
        let name_str = name.as_str();
        if HOP_BY_HOP.contains(&name_str) || nominated.iter().any(|token| token == name_str) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::channel::disposition::Disposition;

    fn outcome(headers: HeaderMap, body: ResponseBody) -> ExecOutcome {
        ExecOutcome {
            status: StatusCode::OK,
            headers,
            body,
            disposition: Disposition::Success,
        }
    }

    #[test]
    fn metadata_sanitizes_and_preserves_end_to_end_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONNECTION,
            "keep-alive, x-internal".parse().unwrap(),
        );
        headers.insert("x-internal", "secret".parse().unwrap());
        headers.insert(header::CONTENT_LENGTH, "99".parse().unwrap());
        headers.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
        headers.append("set-cookie", "a=1".parse().unwrap());
        headers.append("set-cookie", "b=2".parse().unwrap());
        headers.insert("x-gproxy-request-id", "upstream".parse().unwrap());

        let metadata = metadata(
            &outcome(headers, ResponseBody::Full(Bytes::from_static(b"\0\xff"))),
            "request-1",
        );

        assert!(metadata.headers.get(header::CONNECTION).is_none());
        assert!(metadata.headers.get("x-internal").is_none());
        assert!(metadata.headers.get(header::CONTENT_LENGTH).is_none());
        assert_eq!(
            metadata.headers[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        assert_eq!(metadata.headers[header::CACHE_CONTROL], "no-cache");
        assert_eq!(metadata.headers.get_all("set-cookie").iter().count(), 2);
        assert_eq!(metadata.headers["x-gproxy-request-id"], "request-1");
    }

    #[test]
    fn streaming_success_gets_content_type_only_when_absent() {
        let stream = futures_util::stream::empty();
        let metadata = metadata(
            &outcome(HeaderMap::new(), ResponseBody::Stream(Box::pin(stream))),
            "request-2",
        );

        assert_eq!(metadata.headers[header::CONTENT_TYPE], "text/event-stream");
    }
}
