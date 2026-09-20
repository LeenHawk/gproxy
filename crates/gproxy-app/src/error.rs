//! One error type for the whole product layer, and the two things every host
//! needs from it: an HTTP status and a stable machine-readable code.
//!
//! The code is part of the API envelope, so it is a contract: it may gain new
//! values, but an existing string must not change meaning. It is deliberately
//! coarser than the underlying error — a client branches on `forbidden`, it
//! does not branch on which of core's twenty variants produced it.

use gproxy_cache::CacheError;
use gproxy_core::CoreError;
use gproxy_protocol::transform::TransformErrorKind;
use gproxy_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Core(#[from] CoreError),
    /// No usable credential was presented, or the one presented is not valid.
    /// The `&'static str` is a reason for the operator's log, never for the
    /// caller: telling an unauthenticated client why it failed is a probe.
    #[error("unauthorized")]
    Unauthorized(&'static str),
    /// Authenticated, but not entitled. Unlike `Unauthorized`, the reason is
    /// the caller's own policy and is safe to return.
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("{entity} `{id}` not found")]
    NotFound { entity: &'static str, id: String },
    #[error("conflict: {0}")]
    Conflict(String),
    /// A configured rate limit rejected the request before any upstream work.
    /// `retry_after_ms` is None when the window end is not known.
    #[error("rate limited")]
    RateLimited { retry_after_ms: Option<i64> },
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// The HTTP status a host answers with. 5xx means the instance failed;
    /// 4xx means the request did.
    pub fn status_code(&self) -> u16 {
        use http::StatusCode as S;
        let status = match self {
            Self::Store(_) | Self::Internal(_) => S::INTERNAL_SERVER_ERROR,
            Self::Cache(_) => S::SERVICE_UNAVAILABLE,
            Self::Core(error) => return core_status(error),
            Self::Unauthorized(_) => S::UNAUTHORIZED,
            Self::Forbidden(_) => S::FORBIDDEN,
            Self::Invalid(_) => S::BAD_REQUEST,
            Self::NotFound { .. } => S::NOT_FOUND,
            Self::Conflict(_) => S::CONFLICT,
            Self::RateLimited { .. } => S::TOO_MANY_REQUESTS,
        };
        status.as_u16()
    }

    /// Stable machine-readable code for the API envelope.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Store(_) => "store_error",
            Self::Cache(_) => "cache_unavailable",
            Self::Core(error) => core_code(error),
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Invalid(_) => "invalid_request",
            Self::NotFound { .. } => "not_found",
            Self::Conflict(_) => "conflict",
            Self::RateLimited { .. } => "rate_limited",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

/// Core's failures seen from the edge. An exhausted budget is 429 rather than
/// 403 because it is a limit that reopens, and a continuation held by another
/// instance is 421: the request is fine, this instance is the wrong one.
fn core_status(error: &CoreError) -> u16 {
    use http::StatusCode as S;
    let status = match error {
        CoreError::Forbidden(_) => S::FORBIDDEN,
        CoreError::InvalidTarget(_) | CoreError::OperationMismatch { .. } => S::BAD_REQUEST,
        CoreError::NoUsableCredential | CoreError::CredentialDead { .. } => S::SERVICE_UNAVAILABLE,
        CoreError::BudgetExhausted { .. } => S::TOO_MANY_REQUESTS,
        CoreError::ContinuationElsewhere { .. } => S::MISDIRECTED_REQUEST,
        CoreError::RefreshContended { .. } => S::SERVICE_UNAVAILABLE,
        CoreError::Cancelled => S::from_u16(499).unwrap_or(S::BAD_REQUEST),
        CoreError::DeadlineExceeded => S::GATEWAY_TIMEOUT,
        CoreError::NotImplemented(_) => S::NOT_IMPLEMENTED,
        CoreError::Route(_) => S::NOT_FOUND,
        CoreError::Transform(error) => match error.kind() {
            TransformErrorKind::InvalidInput
            | TransformErrorKind::MissingMetadata
            | TransformErrorKind::MissingState => S::BAD_REQUEST,
            TransformErrorKind::Unsupported => S::NOT_IMPLEMENTED,
            TransformErrorKind::Conflict => S::CONFLICT,
            TransformErrorKind::Limit => S::PAYLOAD_TOO_LARGE,
            _ => S::BAD_GATEWAY,
        },
        CoreError::Cache(_) => S::SERVICE_UNAVAILABLE,
        _ => S::INTERNAL_SERVER_ERROR,
    };
    status.as_u16()
}

fn core_code(error: &CoreError) -> &'static str {
    match error {
        CoreError::Forbidden(_) => "forbidden",
        CoreError::InvalidTarget(_) | CoreError::OperationMismatch { .. } => "invalid_request",
        CoreError::NoUsableCredential => "no_usable_credential",
        CoreError::CredentialDead { .. } => "credential_dead",
        CoreError::BudgetExhausted { .. } => "budget_exhausted",
        CoreError::ContinuationElsewhere { .. } => "continuation_elsewhere",
        CoreError::RefreshContended { .. } => "refresh_contended",
        CoreError::Cancelled => "cancelled",
        CoreError::DeadlineExceeded => "deadline_exceeded",
        CoreError::NotImplemented(_) => "not_implemented",
        CoreError::Route(_) => "unknown_model",
        CoreError::Transform(_) => "transform_failed",
        CoreError::Cache(_) => "cache_unavailable",
        CoreError::Store(_) => "store_error",
        _ => "upstream_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_codes_are_stable() {
        assert_eq!(AppError::Unauthorized("no key").status_code(), 401);
        assert_eq!(AppError::Unauthorized("no key").code(), "unauthorized");
        assert_eq!(AppError::forbidden("nope").status_code(), 403);
        assert_eq!(AppError::invalid("bad").status_code(), 400);
        assert_eq!(AppError::not_found("user", "u1").status_code(), 404);
        assert_eq!(AppError::Conflict("dup".into()).status_code(), 409);
        assert_eq!(
            AppError::RateLimited {
                retry_after_ms: Some(10)
            }
            .status_code(),
            429
        );
        assert_eq!(AppError::internal("boom").status_code(), 500);
    }

    #[test]
    fn unauthorized_never_reveals_its_reason() {
        let message = AppError::Unauthorized("api key digest is unknown").to_string();
        assert_eq!(message, "unauthorized");
    }

    #[test]
    fn core_failures_keep_their_own_meaning() {
        let error = AppError::Core(CoreError::BudgetExhausted {
            quota_id: "q".into(),
            window_key: "w".into(),
            resets_at_ms: None,
        });
        assert_eq!(error.status_code(), 429);
        assert_eq!(error.code(), "budget_exhausted");

        let error = AppError::Core(CoreError::NoUsableCredential);
        assert_eq!(error.status_code(), 503);
        assert_eq!(error.code(), "no_usable_credential");
    }
}
