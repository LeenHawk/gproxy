//! Raw AWS event-stream classification, before a channel translates its payloads.
use super::aws_eventstream::FrameParser;
use crate::channel::{
    ResponseReason, ResponseReasonObserver,
    reason::{classify, code_reason},
    standard_reason_observer,
};
use base64::Engine as _;
use http::HeaderMap;
use serde_json::Value;

pub(crate) fn observer(
    headers: &HeaderMap,
    max_bytes: u64,
    bedrock: bool,
) -> Option<Box<dyn ResponseReasonObserver>> {
    if headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';').next().unwrap_or_default().trim() == "application/vnd.amazon.eventstream"
        })
    {
        Some(Box::new(AwsObserver {
            parser: FrameParser::new(),
            reason: None,
            bedrock,
        }))
    } else {
        standard_reason_observer(headers, max_bytes)
    }
}
struct AwsObserver {
    parser: FrameParser,
    reason: Option<ResponseReason>,
    bedrock: bool,
}
impl ResponseReasonObserver for AwsObserver {
    fn observe(&mut self, chunk: &[u8]) {
        let Ok(frames) = self.parser.push(chunk) else {
            return;
        };
        for frame in frames {
            if let Some(code) = frame.exception_type.as_deref() {
                self.reason = Some(code_reason(code).unwrap_or(ResponseReason::UpstreamError));
            }
            if let Some(reason) = frame.event_type.as_deref().and_then(code_reason) {
                self.reason = Some(reason);
            }
            let Ok(mut value) = serde_json::from_slice::<Value>(&frame.payload) else {
                continue;
            };
            if self.bedrock
                && let Some(encoded) = value.get("bytes").and_then(Value::as_str)
            {
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
                    continue;
                };
                let Ok(decoded) = serde_json::from_slice(&bytes) else {
                    continue;
                };
                value = decoded;
            }
            if let Some(reason) = classify(&value) {
                self.reason = Some(reason);
            }
        }
    }
    fn finish(self: Box<Self>) -> Option<ResponseReason> {
        self.reason
    }
}
