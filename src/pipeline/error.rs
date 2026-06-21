//! Pipeline error → HTTP response mapping (handler boundary).

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::channel::ChannelError;

/// Errors surfaced by the request pipeline, each with a fixed HTTP status.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("unknown route: {0}")]
    UnknownRoute(String),
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("unsupported path")]
    UnsupportedPath,
    #[error("no available members")]
    NoMembers,
    #[error("no available credentials")]
    NoCredentials,
    #[error("unknown channel: {0}")]
    UnknownChannel(String),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error("all upstream attempts failed")]
    AllAttemptsFailed,
    #[error("upstream transport error: {0}")]
    Transport(String),
    #[error("request transform failed: {0}")]
    TransformRequest(crate::transform::TransformError),
    #[error("response transform failed: {0}")]
    TransformResponse(crate::transform::TransformError),
    #[error("operation not supported by provider routing rules")]
    RuleUnsupported,
    #[error("local implementation pending")]
    LocalUnimplemented,
    #[error("forbidden")]
    Forbidden,
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("rate-limit/quota counters unavailable")]
    CounterUnavailable,
}

impl PipelineError {
    pub fn status(&self) -> StatusCode {
        use PipelineError::*;
        match self {
            Unauthorized => StatusCode::UNAUTHORIZED,
            UnknownRoute(_) | UnknownProvider(_) | UnsupportedPath => StatusCode::NOT_FOUND,
            NoMembers | NoCredentials => StatusCode::SERVICE_UNAVAILABLE,
            UnknownChannel(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Channel(_) | AllAttemptsFailed | Transport(_) => StatusCode::BAD_GATEWAY,
            TransformRequest(_) => StatusCode::UNPROCESSABLE_ENTITY,
            TransformResponse(_) => StatusCode::BAD_GATEWAY,
            RuleUnsupported | LocalUnimplemented => StatusCode::NOT_IMPLEMENTED,
            Forbidden => StatusCode::FORBIDDEN,
            RateLimited { .. } | QuotaExceeded => StatusCode::TOO_MANY_REQUESTS,
            // The limiter backend is down: enforced limits/quotas can't be
            // checked, so the request is refused (fail-closed), not waved through.
            CounterUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// `Retry-After` value (seconds) for rate-limited errors, else `None`.
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            PipelineError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }

    /// Client-facing JSON error body. Variants whose `Display` embeds
    /// upstream/internal detail (URLs, transport causes, possibly proxy info)
    /// collapse to a generic message and log the real cause server-side
    /// (CWE-209); the rest reveal nothing sensitive. Shared by the native axum
    /// [`IntoResponse`] and the edge fetch entry, so both surfaces redact
    /// identically.
    pub fn error_json(&self) -> String {
        if matches!(
            self,
            PipelineError::Channel(_)
                | PipelineError::Transport(_)
                | PipelineError::AllAttemptsFailed
                | PipelineError::TransformResponse(_)
        ) {
            tracing::warn!(error = %self, "upstream request failed");
        }
        self.error_body_json()
    }

    /// The client-facing JSON error body WITHOUT logging side effects. Byte-for-
    /// byte identical to what [`error_json`](Self::error_json) emits; used by
    /// `execute` to capture the downstream response body without firing the
    /// `tracing::warn` a second time (the real render site already logs it).
    pub fn error_body_json(&self) -> String {
        let message = match self {
            PipelineError::Channel(_)
            | PipelineError::Transport(_)
            | PipelineError::AllAttemptsFailed
            | PipelineError::TransformResponse(_) => "upstream request failed".to_string(),
            other => other.to_string(),
        };
        json!({ "error": { "message": message, "type": "gproxy_error" } }).to_string()
    }
}

impl IntoResponse for PipelineError {
    fn into_response(self) -> Response {
        let status = self.status();
        let retry = self.retry_after_secs();
        let mut resp = (status, self.error_json()).into_response();
        let h = resp.headers_mut();
        h.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        if let Some(secs) = retry
            && let Ok(v) = header::HeaderValue::from_str(&secs.to_string())
        {
            h.insert(header::RETRY_AFTER, v);
        }
        resp
    }
}
