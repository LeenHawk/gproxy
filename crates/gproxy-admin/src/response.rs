use bytes::Bytes;
use http::{HeaderValue, Response, StatusCode};
use serde::Serialize;

use crate::AdminError;
use crate::dto::{ErrorBody, ErrorEnvelope};

pub(crate) fn json(
    status: StatusCode,
    value: &impl Serialize,
) -> Result<Response<Bytes>, AdminError> {
    let body = serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|error| AdminError::Internal(error.to_string()))?;
    Ok(with_body(status, body))
}

pub(crate) fn empty(status: StatusCode) -> Response<Bytes> {
    let mut response = Response::new(Bytes::new());
    *response.status_mut() = status;
    response
}

pub(crate) fn error(error: &AdminError) -> Response<Bytes> {
    let envelope = ErrorEnvelope {
        error: ErrorBody {
            message: error.public_message(),
        },
    };
    json(error.status(), &envelope)
        .unwrap_or_else(|_| with_body(StatusCode::INTERNAL_SERVER_ERROR, Bytes::new()))
}

pub(crate) fn render(
    result: Result<Response<Bytes>, AdminError>,
    surface: &'static str,
) -> Response<Bytes> {
    match result {
        Ok(response) => response,
        Err(error) => {
            if matches!(error, AdminError::Internal(_)) {
                tracing::error!(surface, error = %error, "control dispatch failed");
            }
            self::error(&error)
        }
    }
}

pub(crate) fn no_store(response: &mut Response<Bytes>) {
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
}

fn with_body(status: StatusCode, body: Bytes) -> Response<Bytes> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    no_store(&mut response);
    response
}
