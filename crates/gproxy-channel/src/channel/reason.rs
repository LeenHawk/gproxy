//! Channel-owned response classification, independent of token metering.
//! Only structured response fields are inspected; generated prose is never matched.
use gproxy_protocol::codec::{CodecLimits, JsonDecoder, NdjsonDecoder, SseDecoder, SseFrame};
use http::HeaderMap;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseReason {
    Refusal,
    ContentFilter,
    Safety,
    Blocklist,
    ProhibitedContent,
    SensitivePersonalInformation,
    Recitation,
    Cyber,
    Bio,
    FrontierLlm,
    ReasoningExtraction,
    GeneralHarms,
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
            Self::Safety => "safety",
            Self::Blocklist => "blocklist",
            Self::ProhibitedContent => "prohibited_content",
            Self::SensitivePersonalInformation => "sensitive_personal_information",
            Self::Recitation => "recitation",
            Self::Cyber => "cyber",
            Self::Bio => "bio",
            Self::FrontierLlm => "frontier_llm",
            Self::ReasoningExtraction => "reasoning_extraction",
            Self::GeneralHarms => "general_harms",
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
        | "guardrail_intervened" => ContentFilter,
        "SAFETY" | "IMAGE_SAFETY" => Safety,
        "BLOCKLIST" => Blocklist,
        "PROHIBITED_CONTENT" | "IMAGE_PROHIBITED_CONTENT" => ProhibitedContent,
        "SPII" => SensitivePersonalInformation,
        "RECITATION" | "IMAGE_RECITATION" => Recitation,
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
fn claude_refusal_reason(details: &Value) -> Option<ResponseReason> {
    if details.get("type").and_then(Value::as_str) != Some("refusal") {
        return None;
    }
    use ResponseReason::*;
    Some(match details.get("category").and_then(Value::as_str) {
        Some("cyber") => Cyber,
        Some("bio") => Bio,
        Some("frontier_llm") => FrontierLlm,
        Some("reasoning_extraction") => ReasoningExtraction,
        Some("general_harms") => GeneralHarms,
        _ => Refusal,
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
    // Claude documents policy categories on refusal details, including message_delta.
    // Read these before the generic stop_reason=refusal so the category is retained.
    for path in ["/stop_details", "/delta/stop_details"] {
        if let Some(details) = value.pointer(path)
            && let Some(reason) = claude_refusal_reason(details)
        {
            return Some(reason);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_refusal_categories_precede_generic_stop_reason() {
        for (category, expected) in [
            ("cyber", "cyber"),
            ("bio", "bio"),
            ("frontier_llm", "frontier_llm"),
            ("reasoning_extraction", "reasoning_extraction"),
            ("general_harms", "general_harms"),
            ("future_category", "refusal"),
        ] {
            let message = json!({
                "stop_reason": "refusal",
                "stop_details": {"type": "refusal", "category": category}
            });
            assert_eq!(
                classify(&message).map(ResponseReason::as_str),
                Some(expected)
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                "text/event-stream".parse().unwrap(),
            );
            let mut observer = standard_reason_observer(&headers, 4096).unwrap();
            let event = format!(
                "data: {}\n\n",
                json!({"type": "message_delta", "delta": message})
            );
            for chunk in event.as_bytes().chunks(5) {
                observer.observe(chunk);
            }
            assert_eq!(
                observer.finish().map(ResponseReason::as_str),
                Some(expected)
            );
        }
        for category in [Value::Null, json!("future_category")] {
            assert_eq!(
                classify(&json!({"stop_details": {"type": "refusal", "category": category}})),
                Some(ResponseReason::Refusal)
            );
        }
        // Categories in request echoes, tool inputs or fallback history are not
        // evidence that the final response was refused.
        for value in [
            json!({"category": "frontier_llm"}),
            json!({"input": {"stop_details": {"type": "refusal", "category": "cyber"}}}),
            json!({"stop_reason": "end_turn", "content": [{"type": "fallback", "trigger": {"type": "refusal", "category": "cyber"}}]}),
            json!({"stop_details": {"type": "other", "category": "cyber"}}),
        ] {
            assert_eq!(classify(&value), None);
        }
    }

    #[test]
    fn policy_codes_keep_specific_reasons_across_protocol_shapes() {
        for (code, expected) in [
            ("SAFETY", "safety"),
            ("IMAGE_SAFETY", "safety"),
            ("BLOCKLIST", "blocklist"),
            ("PROHIBITED_CONTENT", "prohibited_content"),
            ("IMAGE_PROHIBITED_CONTENT", "prohibited_content"),
            ("SPII", "sensitive_personal_information"),
            ("RECITATION", "recitation"),
            ("IMAGE_RECITATION", "recitation"),
            ("guardrail_intervened", "content_filter"),
            ("content_filter", "content_filter"),
            ("refusal", "refusal"),
        ] {
            for value in [
                json!({"promptFeedback": {"blockReason": code}}),
                json!({"response": {"candidates": [{"finishReason": code}]}}),
                json!({"stopReason": code}),
                json!({"error": {"code": code, "type": "api_error"}}),
            ] {
                assert_eq!(classify(&value).map(ResponseReason::as_str), Some(expected));
                let mut headers = HeaderMap::new();
                headers.insert(
                    http::header::CONTENT_TYPE,
                    "text/event-stream".parse().unwrap(),
                );
                let mut observer = standard_reason_observer(&headers, 4096).unwrap();
                let event = format!("data: {value}\n\n");
                for chunk in event.as_bytes().chunks(7) {
                    observer.observe(chunk);
                }
                assert_eq!(
                    observer.finish().map(ResponseReason::as_str),
                    Some(expected)
                );
            }
        }
        assert_eq!(
            classify(&json!({"content": [{"type": "text", "text": "SPII SAFETY refusal"}]})),
            None
        );
    }
}
