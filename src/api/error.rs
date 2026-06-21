//! Admin API error → JSON. Mirrors the gateway error shape; never leaks secrets.

use http::StatusCode;

/// Admin-API error taxonomy. Each variant maps to a fixed status and a
/// public-safe message; `Internal` collapses to a generic message and logs the
/// real cause (CWE-209).
#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden(String),
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
    /// 429 Too Many Requests. The inner string is the `Retry-After` value in
    /// seconds (e.g. "60"). Used by the login throttle.
    TooManyRequests(String),
    /// 501 Not Implemented. Used for endpoints that are intentionally absent on
    /// the edge build (e.g. cookie-login, self-update, credential-usage).
    NotImplemented(String),
}

impl ApiError {
    /// HTTP status for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            ApiError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        }
    }

    /// Public-safe message. `Internal` never reveals its cause — that is logged
    /// server-side instead.
    pub fn message(&self) -> String {
        match self {
            ApiError::Unauthorized => "unauthorized".to_string(),
            ApiError::Forbidden(m) => m.clone(),
            ApiError::BadRequest(m) | ApiError::NotFound(m) | ApiError::Conflict(m) => m.clone(),
            ApiError::Internal(cause) => {
                tracing::error!(error = %cause, "admin api internal error");
                "internal error".to_string()
            }
            ApiError::TooManyRequests(_) => "too many login attempts".to_string(),
            ApiError::NotImplemented(m) => m.clone(),
        }
    }

    /// Short machine-readable type tag for the JSON body.  Cross-target so
    /// both the native `IntoResponse` and the edge dispatcher share one mapping.
    pub fn type_str(&self) -> &'static str {
        match self {
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::BadRequest(_) => "bad_request",
            ApiError::NotFound(_) => "not_found",
            ApiError::Conflict(_) => "conflict",
            ApiError::Internal(_) => "internal",
            ApiError::TooManyRequests(_) => "too_many_requests",
            ApiError::NotImplemented(_) => "not_implemented",
        }
    }

    /// Cross-target render of the error envelope `{"error":{"message","type"}}`.
    /// The native `IntoResponse` delegates here; the edge dispatcher uses it
    /// directly (no axum).
    pub fn to_parts(&self) -> (http::StatusCode, Vec<u8>) {
        let body = serde_json::json!({
            "error": { "message": self.message(), "type": self.type_str() }
        });
        (self.status(), serde_json::to_vec(&body).unwrap_or_default())
    }

    /// Extra response headers for this error (e.g. `Retry-After` for 429).
    pub fn extra_headers(&self) -> Vec<(http::HeaderName, http::HeaderValue)> {
        match self {
            ApiError::TooManyRequests(retry_after) => {
                if let Ok(v) = http::HeaderValue::from_str(retry_after) {
                    vec![(http::header::RETRY_AFTER, v)]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// Map an upsert error: a unique-constraint conflict (a `ConflictError`
    /// carried in `anyhow`) becomes 409 with its human-readable message;
    /// anything else is a generic 500. Cross-target so the native CRUD handlers
    /// and the edge dispatcher map `ConflictError` identically.
    pub fn from_upsert(e: anyhow::Error) -> Self {
        match e.downcast_ref::<crate::store::persistence::ConflictError>() {
            Some(c) => ApiError::Conflict(c.0.clone()),
            None => ApiError::Internal(e.to_string()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let extra = self.extra_headers();
        let (status, bytes) = self.to_parts();
        let body = axum::body::Body::from(bytes);
        let mut resp = axum::response::Response::new(body);
        *resp.status_mut() = status;
        resp.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        for (name, value) in extra {
            resp.headers_mut().insert(name, value);
        }
        resp
    }
}
