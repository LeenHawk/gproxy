use serde::{Deserialize, Serialize};

use super::JsonObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ErrorResponse {
    #[serde(rename = "type")]
    pub type_: ErrorResponseType,
    pub error: ErrorBody,
    pub request_id: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorResponseType {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ErrorBody {
    InvalidRequest(InvalidRequestError),
    Authentication(AuthenticationError),
    Billing(BillingError),
    Permission(PermissionError),
    NotFound(NotFoundError),
    RateLimit(RateLimitError),
    GatewayTimeout(GatewayTimeoutError),
    Api(ApiError),
    Overloaded(OverloadedError),
    Unknown(UnknownError),
}

macro_rules! api_error {
    ($name:ident, $type_name:ident, $variant:ident, $wire:literal) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub message: String,
            #[serde(rename = "type")]
            pub type_: $type_name,
            #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
            pub rest: serde_json::Map<String, serde_json::Value>,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum $type_name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

api_error!(
    InvalidRequestError,
    InvalidRequestErrorType,
    InvalidRequestError,
    "invalid_request_error"
);

api_error!(
    AuthenticationError,
    AuthenticationErrorType,
    AuthenticationError,
    "authentication_error"
);

api_error!(
    BillingError,
    BillingErrorType,
    BillingError,
    "billing_error"
);

api_error!(
    PermissionError,
    PermissionErrorType,
    PermissionError,
    "permission_error"
);

api_error!(
    NotFoundError,
    NotFoundErrorType,
    NotFoundError,
    "not_found_error"
);

api_error!(
    RateLimitError,
    RateLimitErrorType,
    RateLimitError,
    "rate_limit_error"
);

api_error!(
    GatewayTimeoutError,
    GatewayTimeoutErrorType,
    GatewayTimeoutError,
    "timeout_error"
);

api_error!(ApiError, ApiErrorType, ApiError, "api_error");

api_error!(
    OverloadedError,
    OverloadedErrorType,
    OverloadedError,
    "overloaded_error"
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UnknownError {
    pub message: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
