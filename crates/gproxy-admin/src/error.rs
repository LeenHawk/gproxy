use http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("too many authentication attempts; retry shortly")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("internal admin error: {0}")]
    Internal(String),
}

impl AdminError {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn public_message(&self) -> String {
        match self {
            Self::Internal(_) => "internal admin error".into(),
            other => other.to_string(),
        }
    }
}

impl From<gproxy_store::StoreError> for AdminError {
    fn from(error: gproxy_store::StoreError) -> Self {
        match error {
            gproxy_store::StoreError::InvalidData {
                field: "usage query",
                message,
            } => Self::BadRequest(message),
            gproxy_store::StoreError::Database(message)
                if message.to_ascii_lowercase().contains("unique") =>
            {
                Self::Conflict("record conflicts with an existing value".into())
            }
            error => Self::Internal(error.to_string()),
        }
    }
}
