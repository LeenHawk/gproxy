//! Two error envelopes, and why this crate must keep them apart.
//!
//! Everything under `/admin/api`, `/portal/api` and the data plane answers the
//! product envelope:
//!
//! ```json
//! { "error": { "code": "forbidden", "message": "…" } }
//! ```
//!
//! `code` comes straight from [`AppError::code`] and the status from
//! [`AppError::status_code`]; this module decides neither. Its whole job is to
//! turn the one into bytes and the other into a response.
//!
//! The OAuth endpoints answer the RFC 6749 §5.2 envelope instead:
//!
//! ```json
//! { "error": "invalid_grant", "error_description": "…" }
//! ```
//!
//! They are not the same document, and an OAuth client cannot read the first
//! one: RFC 6749 says `error` is a string, so a client that finds an object
//! there sees no code at all and cannot tell "re-run the login" from "keep
//! polling". That is why [`OAuthEnvelope`] exists as a separate type rather
//! than as a flag on the other — an endpoint picks its envelope by which
//! wrapper it returns, and the two cannot be confused at a call site.

use axum::response::{IntoResponse, Response};
use gproxy_app::{AppError, operations::issuer::error_body};
use http::{HeaderValue, StatusCode, header};
use serde::Serialize;

/// The product error envelope. `AppError` and `IntoResponse` are both foreign
/// to this crate, so the binding is a wrapper rather than an implementation on
/// the error itself.
pub struct ErrorResponse(pub AppError);

impl From<AppError> for ErrorResponse {
    fn from(error: AppError) -> Self {
        Self(error)
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    error: EnvelopeBody<'a>,
}

#[derive(Serialize)]
struct EnvelopeBody<'a> {
    code: &'a str,
    message: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let error = self.0;
        let status = status_of(&error);
        // A 5xx is this instance's failure, and its message can quote a DSN, a
        // row or a header. The caller gets the code; the operator gets the
        // text.
        let message = if status.is_server_error() {
            tracing::error!(%error, code = error.code(), "request failed");
            "the instance failed to process this request".to_owned()
        } else {
            error.to_string()
        };
        let body = Envelope {
            error: EnvelopeBody {
                code: error.code(),
                message,
            },
        };
        let mut response = json_response(status, &body);
        // RFC 6585 §4: a client that is told to back off is told how long for.
        if let AppError::RateLimited {
            retry_after_ms: Some(ms),
        } = &error
            && let Ok(value) = HeaderValue::from_str(&ms.div_euclid(1_000).max(1).to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

/// The OAuth error envelope, for the issuer endpoints only.
///
/// Every failure they can raise goes through
/// [`error_body`](gproxy_app::operations::issuer::error_body), including the
/// ones that are not OAuth errors at all: a database that is down still has to
/// be reported as `server_error` rather than as this crate's `store_error`,
/// because an OAuth client has never heard of the second one.
pub struct OAuthEnvelope(pub AppError);

impl From<AppError> for OAuthEnvelope {
    fn from(error: AppError) -> Self {
        Self(error)
    }
}

impl IntoResponse for OAuthEnvelope {
    fn into_response(self) -> Response {
        let error = self.0;
        let status = status_of(&error);
        if status.is_server_error() {
            tracing::error!(%error, "oauth request failed");
        }
        json_response(status, &error_body(&error))
    }
}

/// [`AppError::status_code`] as a `StatusCode`. A code outside the HTTP range
/// cannot be answered with, so it becomes a 500 rather than a panic.
fn status_of(error: &AppError) -> StatusCode {
    StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

/// A JSON body with its status. Serialization of a type this crate owns cannot
/// fail, so the fallback is unreachable rather than a real branch.
pub(crate) fn json_response<T: Serialize>(status: StatusCode, body: &T) -> Response {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::new(axum::body::Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

/// A successful JSON answer. The operations return DTOs; this is the one place
/// that decides they are rendered as `200 application/json` with no wrapper —
/// the envelope above is for failures only.
pub(crate) fn ok_json<T: Serialize>(body: &T) -> Response {
    json_response(StatusCode::OK, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use gproxy_app::operations::issuer::IssuerError;

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn the_product_envelope_carries_the_code_and_the_status() {
        let response = ErrorResponse(AppError::forbidden("no provider")).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = body_of(response).await;
        assert_eq!(body["error"]["code"], "forbidden");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no provider")
        );
    }

    #[tokio::test]
    async fn a_failure_of_this_instance_is_not_quoted_back_to_the_caller() {
        let response =
            ErrorResponse(AppError::internal("dsn postgres://user:hunter2@db")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_of(response).await;
        assert_eq!(body["error"]["code"], "internal_error");
        assert!(
            !body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("hunter2")
        );
    }

    #[tokio::test]
    async fn a_rate_limit_says_how_long_to_wait() {
        let response = ErrorResponse(AppError::RateLimited {
            retry_after_ms: Some(2_500),
        })
        .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "2");
        // A sub-second window still asks for at least one second: `Retry-After:
        // 0` invites an immediate retry into the same closed window.
        let response = ErrorResponse(AppError::RateLimited {
            retry_after_ms: Some(120),
        })
        .into_response();
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
    }

    #[tokio::test]
    async fn the_oauth_envelope_is_a_different_document() {
        let response =
            OAuthEnvelope(IssuerError::invalid_grant("code already used")).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_of(response).await;
        // A string, not an object: RFC 6749 §5.2.
        assert_eq!(body["error"], "invalid_grant");
        assert_eq!(body["error_description"], "code already used");
        assert!(body["error"]["code"].is_null());
    }

    #[tokio::test]
    async fn an_oauth_endpoint_reports_a_store_failure_in_the_protocols_vocabulary() {
        let response = OAuthEnvelope(AppError::internal("db is down")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_of(response).await;
        assert_eq!(body["error"], "server_error");
        assert!(
            !body["error_description"]
                .as_str()
                .unwrap()
                .contains("db is down")
        );
    }
}
