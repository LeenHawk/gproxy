//! Reuse Devin's established taxonomy for unary errors and Connect trailers.
use super::{
    connect::FrameReader,
    error::{self, ErrorClass},
};
use crate::channel::{ResponseReason, ResponseReasonObserver, standard_reason_observer};
use http::{HeaderMap, StatusCode};

pub(super) fn observer(
    status: StatusCode,
    headers: &HeaderMap,
    max_bytes: u64,
) -> Option<Box<dyn ResponseReasonObserver>> {
    let connect = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/connect+"));
    if connect || status.is_client_error() || status.is_server_error() {
        Some(Box::new(Observer {
            reader: connect.then(FrameReader::new),
            body: Vec::new(),
            status,
            reason: None,
        }))
    } else {
        standard_reason_observer(headers, max_bytes)
    }
}
fn classify(status: Option<StatusCode>, bytes: &[u8]) -> Option<ResponseReason> {
    let (code, message) = error::payload(bytes);
    if code.is_none() && message.is_none() {
        return None;
    }
    Some(
        match error::classify(
            message.as_deref().unwrap_or_default(),
            code.as_deref(),
            status,
        )
        .class
        {
            ErrorClass::ClientRequest => ResponseReason::InvalidRequest,
            ErrorClass::ContentBlocked => ResponseReason::ContentFilter,
            ErrorClass::Unauthorized => ResponseReason::AuthenticationFailed,
            ErrorClass::ModelBlocked => ResponseReason::PermissionDenied,
            ErrorClass::RateLimited | ErrorClass::Capacity => ResponseReason::RateLimited,
            ErrorClass::QuotaExhausted => ResponseReason::QuotaExhausted,
            ErrorClass::UpstreamInternal | ErrorClass::Unknown => ResponseReason::UpstreamError,
        },
    )
}
struct Observer {
    reader: Option<FrameReader>,
    body: Vec<u8>,
    status: StatusCode,
    reason: Option<ResponseReason>,
}
impl ResponseReasonObserver for Observer {
    fn observe(&mut self, chunk: &[u8]) {
        if let Some(reader) = &mut self.reader {
            if let Ok(frames) = reader.push(chunk) {
                for frame in frames {
                    if frame.end_stream
                        && let Some(reason) = classify(None, &frame.payload)
                    {
                        self.reason = Some(reason);
                    }
                }
            }
        } else {
            self.body.extend_from_slice(chunk);
        }
    }
    fn finish(self: Box<Self>) -> Option<ResponseReason> {
        self.reason
            .or_else(|| classify(Some(self.status), &self.body))
    }
}
