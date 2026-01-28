use bytes::Bytes;
use http::StatusCode;

#[derive(Debug)]
pub struct ProxyError {
    pub status: StatusCode,
    pub body: Bytes,
}

impl ProxyError {
    pub fn bad_request(message: impl Into<Bytes>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: message.into(),
        }
    }

    pub fn not_found(message: impl Into<Bytes>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: message.into(),
        }
    }

    pub fn method_not_allowed(message: impl Into<Bytes>) -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            body: message.into(),
        }
    }
}
