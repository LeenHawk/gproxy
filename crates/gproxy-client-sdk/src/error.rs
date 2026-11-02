use serde::{Deserialize, Serialize};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ErrorKind {
    Auth,
    PaymentRequired,
    RateLimit,
    Overloaded,
    Timeout,
    NotFound,
    ContextTooLong,
    ContentBlocked,
    Upstream,
    Network,
    InvalidData,
}

/// Classification and observed transport facts are independent. In-band errors
/// have no HTTP status until a client supplies the actual response head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Error {
    pub kind: ErrorKind,
    pub status: Option<u16>,
    pub retryable: bool,
    pub body: Option<String>,
    /// Original value: seconds or an HTTP date.
    pub retry_after: Option<String>,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Code {
    Text(String),
    Number(serde_json::Number),
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Payload {
    code: Option<Code>,
    #[serde(rename = "type")]
    type_: Option<String>,
    status: Option<String>,
    message: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Envelope {
    Nested { error: Payload },
    Direct(Payload),
}

impl Error {
    pub(crate) fn invalid(error: impl std::fmt::Display) -> Self {
        Self {
            kind: ErrorKind::InvalidData,
            status: None,
            retryable: false,
            body: None,
            retry_after: None,
            message: error.to_string(),
        }
    }
    pub(crate) fn network(error: impl std::fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Network,
            status: None,
            retryable: true,
            body: None,
            retry_after: None,
            message: error.to_string(),
        }
    }
    pub(crate) fn network_cause(error: &(dyn std::error::Error + 'static)) -> Self {
        let mut value = Self::network(error);
        let mut cause = Some(error);
        while let Some(error) = cause {
            let timed_out = error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::TimedOut);
            #[cfg(all(feature = "remote", not(target_arch = "wasm32")))]
            let timed_out = timed_out
                || error
                    .downcast_ref::<gproxy_client::reqwest::Error>()
                    .is_some_and(|error| error.is_timeout());
            if timed_out {
                value.kind = ErrorKind::Timeout;
                break;
            }
            cause = error.source();
        }
        value
    }
    pub(crate) fn content_blocked(body: String) -> Self {
        Self {
            kind: ErrorKind::ContentBlocked,
            status: None,
            retryable: false,
            body: Some(body),
            retry_after: None,
            message: "content blocked".into(),
        }
    }
    pub(crate) fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self.retryable |= retry_status(status);
        self
    }
    pub(crate) fn with_head(mut self, status: u16, headers: &http::HeaderMap) -> Self {
        self = self.with_status(status);
        if self.retry_after.is_none() {
            self.retry_after = headers
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
        }
        self
    }
    pub(crate) fn with_body(mut self, body: &[u8]) -> Self {
        if self.body.is_none() {
            self.body = Some(String::from_utf8_lossy(body).into_owned());
        }
        self
    }
    pub fn from_response(status: u16, headers: &http::HeaderMap, body: String) -> Self {
        Self::classify(Some(status), headers, body)
    }
    pub fn from_event(body: String) -> Self {
        Self::classify(None, &http::HeaderMap::new(), body)
    }
    fn classify(status: Option<u16>, headers: &http::HeaderMap, body: String) -> Self {
        let payload = serde_json::from_str::<Envelope>(&body)
            .ok()
            .map(|v| match v {
                Envelope::Nested { error } => error,
                Envelope::Direct(error) => error,
            });
        let native_status = payload.as_ref().and_then(|v| match &v.code {
            Some(Code::Number(n)) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
            _ => None,
        });
        let mut labels = Vec::new();
        if let Some(p) = &payload {
            if let Some(Code::Text(code)) = &p.code {
                labels.push(code.as_str());
            }
            labels.extend(p.type_.as_deref());
            labels.extend(p.status.as_deref());
        }
        let has = |names: &[&str]| labels.iter().any(|label| names.contains(label));
        let effective = status.filter(|s| *s >= 400).or(native_status);
        let kind = if effective == Some(402) || has(&["payment_required", "insufficient_credits"]) {
            ErrorKind::PaymentRequired
        } else if matches!(effective, Some(401 | 403))
            || has(&[
                "authentication_error",
                "invalid_api_key",
                "UNAUTHENTICATED",
                "PERMISSION_DENIED",
            ])
        {
            ErrorKind::Auth
        } else if has(&[
            "overloaded_error",
            "overloaded",
            "provider_overloaded",
            "MODEL_OVERLOADED",
        ]) || effective == Some(529)
        {
            ErrorKind::Overloaded
        } else if matches!(effective, Some(408 | 504 | 524))
            || has(&[
                "timeout",
                "timeout_error",
                "request_timeout",
                "DEADLINE_EXCEEDED",
            ])
        {
            ErrorKind::Timeout
        } else if effective == Some(429)
            || has(&[
                "rate_limit_error",
                "rate_limit_exceeded",
                "RESOURCE_EXHAUSTED",
            ])
        {
            ErrorKind::RateLimit
        } else if effective == Some(404) || has(&["not_found", "not_found_error", "NOT_FOUND"]) {
            ErrorKind::NotFound
        } else if has(&[
            "context_length_exceeded",
            "model_context_window_exceeded",
            "prompt_too_long",
        ]) {
            ErrorKind::ContextTooLong
        } else if has(&[
            "content_filter",
            "content_policy_violation",
            "safety",
            "SAFETY",
        ]) {
            ErrorKind::ContentBlocked
        } else {
            ErrorKind::Upstream
        };
        let retryable = status.is_some_and(retry_status)
            || native_status.is_some_and(retry_status)
            || matches!(
                kind,
                ErrorKind::RateLimit | ErrorKind::Overloaded | ErrorKind::Timeout
            )
            || has(&["server_error", "api_error", "INTERNAL", "UNAVAILABLE"]);
        let retry_after = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let message = payload
            .and_then(|v| v.message)
            .unwrap_or_else(|| match status {
                Some(status) => format!("{kind:?}: HTTP {status}"),
                None => format!("{kind:?}: provider error"),
            });
        Self {
            kind,
            status,
            retryable,
            body: Some(body),
            retry_after,
            message,
        }
    }
}
fn retry_status(status: u16) -> bool {
    matches!(status, 408 | 409 | 429 | 500..=599)
}
impl From<gproxy_protocol::codec::CodecError> for Error {
    fn from(error: gproxy_protocol::codec::CodecError) -> Self {
        if error.kind() == gproxy_protocol::codec::CodecErrorKind::Transport {
            Self::network_cause(&error)
        } else {
            Self::invalid(error)
        }
    }
}
