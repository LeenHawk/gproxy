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
use gproxy_sdk::SdkError;
use gproxy_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Core(#[from] CoreError),
    /// A handle failure the engine did not raise: an unknown model, a plan
    /// with nothing usable left in it, an upstream's own refusal after every
    /// target was tried.
    ///
    /// Kept whole rather than flattened into the variants above, because
    /// [`SdkError::status_code`] has already decided what each of them is
    /// worth on the wire and two layers disagreeing about that is how a 429
    /// becomes a 500. Engine, store and cache failures are the exception: see
    /// the `From` implementation below.
    #[error(transparent)]
    Sdk(SdkError),
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
    /// An OAuth protocol refusal from the issuer, carrying the RFC's own error
    /// code.
    ///
    /// Kept whole rather than flattened into `Invalid` because the code is
    /// what the client branches on: `invalid_grant` means re-run the login,
    /// `authorization_pending` means keep polling, `invalid_request` means the
    /// client has a bug. A host that had to guess between those from an
    /// English message would make a client re-authenticate on a typo, or poll
    /// forever on a real failure. See
    /// [`issuer::error_body`](crate::operations::issuer::error_body), which is
    /// how both hosts render one.
    #[error(transparent)]
    OAuth(#[from] crate::operations::issuer::IssuerError),
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
            Self::Sdk(error) => return error.status_code(),
            Self::Unauthorized(_) => S::UNAUTHORIZED,
            Self::Forbidden(_) => S::FORBIDDEN,
            Self::Invalid(_) => S::BAD_REQUEST,
            Self::OAuth(error) => return error.status_code(),
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
            Self::Sdk(error) => sdk_code(error),
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Invalid(_) => "invalid_request",
            Self::OAuth(error) => error.code.as_str(),
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

/// The handle's failures, seen from the product layer.
///
/// Engine, store and cache failures are unwrapped into the variants this type
/// already has, so a host that matches on `AppError::Core` still sees a spent
/// budget or a dead credential whether it came through `Admission` or through
/// the sdk. Everything else — a name that resolved to nothing, an upstream's
/// own status, a build that cannot serve the request — stays whole, because
/// flattening it would mean re-deciding a status the sdk has already decided.
impl From<SdkError> for AppError {
    fn from(error: SdkError) -> Self {
        match error {
            SdkError::Core(error) => Self::Core(error),
            SdkError::Store(error) => Self::Store(error),
            SdkError::Cache(error) => Self::Cache(error),
            other => Self::Sdk(other),
        }
    }
}

/// Core's failures seen from the edge. An exhausted budget is 429 rather than
/// 403 because it is a limit that reopens, and a continuation held by another
/// instance is 421: the request is fine, this instance is the wrong one.
fn core_status(error: &CoreError) -> u16 {
    use http::StatusCode as S;
    let status = match error {
        CoreError::Forbidden(_) => S::FORBIDDEN,
        CoreError::ResourceNotFound { .. } => S::NOT_FOUND,
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
        CoreError::ResourceNotFound { .. } => "not_found",
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

/// The handle's failures as envelope codes. Exhaustive on purpose: a new
/// `SdkError` variant has to be named here rather than silently becoming an
/// upstream error.
fn sdk_code(error: &SdkError) -> &'static str {
    match error {
        SdkError::Core(error) => core_code(error),
        SdkError::Store(_) | SdkError::Db(_) => "store_error",
        SdkError::Cache(_) => "cache_unavailable",
        SdkError::Invalid(_) => "invalid_request",
        SdkError::NotFound { .. } => "not_found",
        SdkError::Conflict(_) => "conflict",
        SdkError::Unsupported(_) => "not_implemented",
        SdkError::UnknownModel(_) => "unknown_model",
        SdkError::NoTarget(_) => "no_usable_target",
        SdkError::LoginExpired => "login_expired",
        SdkError::Upstream { .. } | SdkError::Channel(_) => "upstream_error",
        SdkError::Build(_) | SdkError::Secret(_) | SdkError::File(_) => "internal_error",
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

    #[test]
    fn the_handles_failures_keep_the_status_the_handle_gave_them() {
        // An upstream's own refusal is the caller's answer, not a 500.
        let error = AppError::from(SdkError::Upstream {
            status: 429,
            body: "provider `p1` answered 429".into(),
        });
        assert_eq!(error.status_code(), 429);
        assert_eq!(error.code(), "upstream_error");

        let error = AppError::from(SdkError::NoTarget("test/m1".into()));
        assert_eq!(error.status_code(), 503);
        assert_eq!(error.code(), "no_usable_target");
        assert!(error.to_string().contains("test/m1"), "{error}");

        let error = AppError::from(SdkError::UnknownModel("nope".into()));
        assert_eq!(error.status_code(), 404);
        assert_eq!(error.code(), "unknown_model");
    }

    #[test]
    fn the_engines_failures_are_unwrapped_rather_than_wrapped_twice() {
        // Matching on `AppError::Core` has to work whether the failure came
        // through admission or through the handle.
        let error = AppError::from(SdkError::Core(CoreError::BudgetExhausted {
            quota_id: "q".into(),
            window_key: "w".into(),
            resets_at_ms: None,
        }));
        assert!(matches!(error, AppError::Core(_)));
        assert_eq!(error.status_code(), 429);
        assert_eq!(error.code(), "budget_exhausted");
    }
}
