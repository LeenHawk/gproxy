//! Channel-owned response classification, independent of token metering.
//! Only structured response fields are inspected; generated prose is never matched.
use gproxy_protocol::codec::{CodecLimits, JsonDecoder, NdjsonDecoder, SseDecoder, SseFrame};
use http::HeaderMap;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseReason {
    Refusal,
    ContentFilter,
    AuthenticationFailed,
    PermissionDenied,
    RateLimited,
    QuotaExhausted,
    InvalidRequest,
    NotFound,
    UpstreamError,
    Timeout,
    ConnectionError,
    Cancelled,
}
impl ResponseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refusal => "refusal",
            Self::ContentFilter => "content_filter",
            Self::AuthenticationFailed => "authentication_failed",
            Self::PermissionDenied => "permission_denied",
            Self::RateLimited => "rate_limited",
            Self::QuotaExhausted => "quota_exhausted",
            Self::InvalidRequest => "invalid_request",
            Self::NotFound => "not_found",
            Self::UpstreamError => "upstream_error",
            Self::Timeout => "timeout",
            Self::ConnectionError => "connection_error",
            Self::Cancelled => "cancelled",
        }
    }
}

/// A channel can replace the standard observer for its private wire format.
/// Chunks are observed before conversion and never retained by Core for this purpose.
pub trait ResponseReasonObserver: Send {
    fn observe(&mut self, chunk: &[u8]);
    fn finish(self: Box<Self>) -> Option<ResponseReason>;
}

pub(crate) fn websocket_reason(
    frame: &gproxy_protocol::connection::WsFrame,
) -> Option<ResponseReason> {
    use gproxy_protocol::connection::WsFrame;
    let bytes = match frame {
        WsFrame::Text(text) => text.as_bytes(),
        WsFrame::Binary(bytes) => bytes.as_ref(),
        WsFrame::Ping(_) | WsFrame::Pong(_) | WsFrame::Close(_) => return None,
    };
    let value: Value = serde_json::from_slice(bytes).ok()?;
    classify(&value)
}

enum Decoder {
    Json(JsonDecoder),
    Sse(SseDecoder),
    Ndjson(NdjsonDecoder),
}
struct StandardObserver {
    decoder: Decoder,
    reason: Option<ResponseReason>,
}

/// Standard OpenAI/Claude/Gemini JSON and SSE, including Code Assist envelopes.
/// Non-JSON transports deliberately return no observer rather than guessing.
pub fn standard_reason_observer(
    headers: &HeaderMap,
    max_bytes: u64,
) -> Option<Box<dyn ResponseReasonObserver>> {
    let content_type = headers
        .get(http::header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .trim();
    let limits = CodecLimits {
        max_buffer_bytes: max_bytes,
        max_value_bytes: max_bytes,
        max_body_bytes: max_bytes,
        max_line_bytes: max_bytes,
        max_part_bytes: 0,
        max_parts: 0,
    };
    let decoder = match content_type {
        "text/event-stream" => Decoder::Sse(SseDecoder::new(limits)),
        "application/x-ndjson" | "application/ndjson" => {
            Decoder::Ndjson(NdjsonDecoder::new(limits))
        }
        t if t == "application/json" || t.ends_with("+json") => {
            Decoder::Json(JsonDecoder::new(limits))
        }
        _ => return None,
    };
    Some(Box::new(StandardObserver {
        decoder,
        reason: None,
    }))
}
impl StandardObserver {
    fn values(&mut self, values: impl IntoIterator<Item = Value>) {
        for value in values {
            if let Some(reason) = classify(&value) {
                self.reason = Some(reason);
            }
        }
    }
}
fn sse_values(frames: Vec<SseFrame>) -> impl Iterator<Item = Value> {
    frames.into_iter().filter_map(|frame| match frame {
        SseFrame::Event(event) => serde_json::from_str(&event.data).ok(),
        SseFrame::Done => None,
    })
}
impl ResponseReasonObserver for StandardObserver {
    fn observe(&mut self, chunk: &[u8]) {
        match &mut self.decoder {
            Decoder::Json(decoder) => {
                let _ = decoder.push(chunk);
            }
            Decoder::Sse(decoder) => {
                if let Ok(frames) = decoder.push(chunk) {
                    self.values(sse_values(frames));
                }
            }
            Decoder::Ndjson(decoder) => {
                if let Ok(values) = decoder.push(chunk) {
                    self.values(values);
                }
            }
        }
    }
    fn finish(self: Box<Self>) -> Option<ResponseReason> {
        let Self {
            decoder,
            mut reason,
        } = *self;
        let values: Vec<Value> = match decoder {
            Decoder::Json(decoder) => decoder.finish().ok().into_iter().collect(),
            Decoder::Sse(mut decoder) => decoder
                .finish()
                .ok()
                .map(|v| sse_values(v).collect())
                .unwrap_or_default(),
            Decoder::Ndjson(mut decoder) => decoder.finish().unwrap_or_default(),
        };
        for value in values {
            if let Some(found) = classify(&value) {
                reason = Some(found);
            }
        }
        reason
    }
}

pub(crate) fn code_reason(code: &str) -> Option<ResponseReason> {
    use ResponseReason::*;
    Some(match code {
        "refusal" => Refusal,
        "content_filter"
        | "content_policy_violation"
        | "ResponsibleAIPolicyViolation"
        | "content_filtered"
        | "guardrail_intervened"
        | "SAFETY"
        | "BLOCKLIST"
        | "PROHIBITED_CONTENT"
        | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT"
        | "IMAGE_RECITATION"
        | "RECITATION"
        | "SPII" => ContentFilter,
        "authentication_error" | "invalid_api_key" | "UNAUTHENTICATED" => AuthenticationFailed,
        "permission_error"
        | "permission_denied"
        | "PERMISSION_DENIED"
        | "AccessDeniedException" => PermissionDenied,
        "rate_limit_error"
        | "rate_limit_exceeded"
        | "RESOURCE_EXHAUSTED"
        | "ThrottlingException"
        | "throttlingException" => RateLimited,
        "insufficient_quota"
        | "quota_exceeded"
        | "credit_balance_too_low"
        | "ServiceQuotaExceededException" => QuotaExhausted,
        "invalid_request_error"
        | "INVALID_ARGUMENT"
        | "ValidationException"
        | "validationException" => InvalidRequest,
        "not_found_error" | "NOT_FOUND" | "ResourceNotFoundException" => NotFound,
        "error"
        | "api_error"
        | "server_error"
        | "overloaded_error"
        | "INTERNAL"
        | "UNAVAILABLE"
        | "InternalServerException"
        | "internalServerException"
        | "ServiceUnavailableException"
        | "serviceUnavailableException"
        | "invalidStateEvent" => UpstreamError,
        _ => return None,
    })
}
pub(crate) fn classify(value: &Value) -> Option<ResponseReason> {
    if let Some(values) = value.as_array() {
        return values.iter().find_map(classify);
    }
    // Error codes take precedence over broad error types, never inspect message prose.
    if let Some(error) = value.get("error") {
        for key in ["code", "type", "status"] {
            if let Some(reason) = error.get(key).and_then(Value::as_str).and_then(code_reason) {
                return Some(reason);
            }
        }
    }
    if value.get("type").and_then(Value::as_str) == Some("error")
        && let Some(reason) = value
            .get("code")
            .and_then(Value::as_str)
            .and_then(code_reason)
    {
        return Some(reason);
    }
    for path in [
        "/stop_reason",
        "/delta/stop_reason",
        "/stopReason",
        "/incomplete_details/reason",
        "/status_details/reason",
        "/promptFeedback/blockReason",
    ] {
        if let Some(reason) = value
            .pointer(path)
            .and_then(Value::as_str)
            .and_then(code_reason)
        {
            return Some(reason);
        }
    }
    if matches!(
        value.get("type").and_then(Value::as_str),
        Some("refusal" | "response.refusal.delta" | "response.refusal.done")
    ) {
        return Some(ResponseReason::Refusal);
    }
    if value
        .get("refusal")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Some(ResponseReason::Refusal);
    }
    for key in ["choices", "candidates"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                for field in ["finish_reason", "finishReason"] {
                    if let Some(reason) = item
                        .get(field)
                        .and_then(Value::as_str)
                        .and_then(code_reason)
                    {
                        return Some(reason);
                    }
                }
                for field in ["message", "delta"] {
                    if let Some(reason) = item.get(field).and_then(classify) {
                        return Some(reason);
                    }
                }
            }
        }
    }
    // Traverse only protocol containers. In particular, never inspect tool arguments,
    // echoed request input, JSON in assistant text, or arbitrary metadata.
    for key in [
        "response",
        "output",
        "content",
        "message",
        "content_block",
        "status_details",
        "part",
        "item",
    ] {
        if let Some(reason) = value
            .get(key)
            .filter(|v| v.is_object() || v.is_array())
            .and_then(classify)
        {
            return Some(reason);
        }
    }
    None
}
