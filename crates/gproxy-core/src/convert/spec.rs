//! Queries over protocol's declarative operation table; protocol itself
//! exposes only the array.

use gproxy_protocol::{
    OperationKey,
    connection::StreamFraming,
    spec::{HttpBodyFormat, OPERATION_SPECS, OperationSpec, OperationTransport},
};
use http::{HeaderMap, header};

pub fn spec_for(key: OperationKey) -> Option<&'static OperationSpec> {
    OPERATION_SPECS.iter().find(|spec| spec.key == key)
}

pub fn is_websocket(key: OperationKey) -> bool {
    matches!(
        spec_for(key).map(|spec| &spec.transport),
        Some(OperationTransport::WebSocket)
    )
}

/// How an upstream response body is framed, or None for a whole body. The
/// Content-Type decides when it is unambiguous; otherwise the operation's
/// declared response formats decide (a spec listing only a JSON array stream
/// means the plain-JSON content type carries array elements).
pub fn response_framing(key: OperationKey, headers: &HeaderMap) -> Option<StreamFraming> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if content_type == "text/event-stream" {
        return Some(StreamFraming::Sse);
    }
    if content_type == "application/x-ndjson" || content_type == "application/jsonl" {
        return Some(StreamFraming::NdJson);
    }
    let Some(OperationTransport::Http { response, .. }) = spec_for(key).map(|spec| &spec.transport)
    else {
        return None;
    };
    let declares_plain_json = response.contains(&HttpBodyFormat::Json);
    let array = response.contains(&HttpBodyFormat::JsonStream(StreamFraming::JsonArray));
    let ndjson = response.contains(&HttpBodyFormat::JsonStream(StreamFraming::NdJson));
    if content_type == "application/json" || content_type.is_empty() {
        if array && !declares_plain_json {
            return Some(StreamFraming::JsonArray);
        }
        if ndjson && !declares_plain_json {
            return Some(StreamFraming::NdJson);
        }
    }
    None
}
