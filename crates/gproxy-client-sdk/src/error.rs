use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Error {
    #[error("authentication failed ({status})")]
    Auth { status: u16, body: String },
    /// Kept as the original HTTP value: either seconds or an HTTP date.
    #[error("rate limited")]
    RateLimit {
        retry_after: Option<String>,
        body: String,
    },
    #[error("context too long")]
    ContextTooLong { body: String },
    #[error("content blocked")]
    ContentBlocked { body: String },
    #[error("upstream returned {status}: {body}")]
    Upstream { status: u16, body: String },
    #[error("network: {message}")]
    Network { message: String },
    /// Invalid external data (including malformed streams), or parameters the
    /// native wire types cannot represent. No type-level invariants are checked.
    #[error("invalid data: {message}")]
    InvalidData { message: String },
}

impl From<gproxy_protocol::codec::CodecError> for Error {
    fn from(error: gproxy_protocol::codec::CodecError) -> Self {
        if error.kind() == gproxy_protocol::codec::CodecErrorKind::Transport {
            Self::network(error)
        } else {
            Self::invalid(error)
        }
    }
}
impl Error {
    pub(crate) fn invalid(error: impl std::fmt::Display) -> Self {
        Self::InvalidData {
            message: error.to_string(),
        }
    }
    pub(crate) fn network(error: impl std::fmt::Display) -> Self {
        Self::Network {
            message: error.to_string(),
        }
    }
    pub fn from_response(status: u16, headers: &http::HeaderMap, body: String) -> Self {
        let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let error = value.get("error").unwrap_or(&value);
        let code = ["code", "type", "status"]
            .into_iter()
            .find_map(|key| error[key].as_str())
            .unwrap_or("");
        match (status, code) {
            (401 | 403, _)
            | (_, "authentication_error" | "invalid_api_key" | "UNAUTHENTICATED") => {
                Self::Auth { status, body }
            }
            (429, _) | (_, "rate_limit_error" | "rate_limit_exceeded" | "RESOURCE_EXHAUSTED") => {
                Self::RateLimit {
                    retry_after: headers
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned),
                    body,
                }
            }
            (
                _,
                "context_length_exceeded" | "model_context_window_exceeded" | "prompt_too_long",
            ) => Self::ContextTooLong { body },
            (_, "content_filter" | "content_policy_violation" | "safety") => {
                Self::ContentBlocked { body }
            }
            _ => Self::Upstream { status, body },
        }
    }
}
