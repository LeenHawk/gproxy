use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// API error type that converts to HTTP responses.
#[derive(Debug)]
pub struct HttpError {
    pub status: StatusCode,
    pub message: String,
}

impl HttpError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let body = axum::Json(ErrorBody {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

impl From<sea_orm::DbErr> for HttpError {
    fn from(err: sea_orm::DbErr) -> Self {
        tracing::error!(error = %err, "database error");
        Self::internal("internal database error")
    }
}

impl From<gproxy_sdk::channel::response::UpstreamError> for HttpError {
    fn from(err: gproxy_sdk::channel::response::UpstreamError) -> Self {
        // Log full error details internally, return generic message to client
        tracing::error!(error = %err, "upstream provider error");
        Self::internal("upstream provider error")
    }
}

impl From<gproxy_sdk::engine::engine::ExecuteError> for HttpError {
    fn from(err: gproxy_sdk::engine::engine::ExecuteError) -> Self {
        // `ExecuteError` wraps an `UpstreamError` with optional attempt
        // diagnostics; the diagnostics are meant for the DB log, not the
        // client response, so the HTTP conversion just forwards the
        // inner error through the existing `From<UpstreamError>` path.
        tracing::error!(error = %err.error, "upstream provider error");
        Self::internal("upstream provider error")
    }
}

/// Standard acknowledgment response for mutation operations.
#[derive(Serialize)]
pub struct AckResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}
