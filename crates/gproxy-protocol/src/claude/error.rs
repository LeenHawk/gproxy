use serde::{Deserialize, Serialize};

use crate::claude::types::RequestId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorResponseTypeKnown {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorResponseType {
    Known(ErrorResponseTypeKnown),
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorTypeKnown {
    /// 400
    #[serde(rename = "invalid_request_error")]
    InvalidRequestError,
    /// 401
    #[serde(rename = "authentication_error")]
    AuthenticationError,
    /// 403
    #[serde(rename = "permission_error")]
    PermissionError,
    /// 404
    #[serde(rename = "not_found_error")]
    NotFoundError,
    /// 413
    #[serde(rename = "request_too_large")]
    RequestTooLarge,
    /// 429
    #[serde(rename = "rate_limit_error")]
    RateLimitError,
    /// 500
    #[serde(rename = "api_error")]
    ApiError,
    /// 529
    #[serde(rename = "overloaded_error")]
    OverloadedError,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorType {
    Known(ErrorTypeKnown),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub r#type: ErrorType,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    #[serde(rename = "type")]
    pub r#type: ErrorResponseType,
    pub error: ErrorDetail,
    pub request_id: RequestId,
}
