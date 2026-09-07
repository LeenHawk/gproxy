//! Classified core errors and their framework-free rendering.
//!
//! Hosts own the final response type: `IntoResponse` impls live in host
//! crates, never here (v2 leaked axum into the wasm build this way). The
//! helpers below give every host the same status and JSON body to render.

use http::StatusCode;

use gproxy_channel_api::{ChannelError, StateError};

/// Wire transport failures — defined at the contract layer, re-exported
/// here so `CoreError::Transport` and host code share one type.
pub use gproxy_channel_api::TransportError;

impl From<gproxy_transform::TransformError> for CoreError {
    fn from(error: gproxy_transform::TransformError) -> Self {
        Self::Transform(error.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("unknown route or model: {0}")]
    UnknownRoute(String),
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("unsupported path or operation")]
    Unsupported,
    #[error("rate limited")]
    RateLimited { retry_after_secs: u32 },
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("no usable credential")]
    NoCredentials,
    #[error("protocol transform failed: {0}")]
    Transform(String),
    #[error("all upstream attempts failed: {0}")]
    UpstreamExhausted(String),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error(transparent)]
    SurfaceState(#[from] StateError),
    #[error("internal: {0}")]
    Internal(String),
}

impl CoreError {
    /// The HTTP status a host should answer with.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::UnknownRoute(_) | Self::UnknownProvider(_) => StatusCode::NOT_FOUND,
            Self::Unsupported => StatusCode::BAD_REQUEST,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::QuotaExceeded => StatusCode::PAYMENT_REQUIRED,
            Self::NoCredentials | Self::UpstreamExhausted(_) => StatusCode::BAD_GATEWAY,
            Self::Transform(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Transport(_) => StatusCode::BAD_GATEWAY,
            Self::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Channel(
                ChannelError::Secret(_) | ChannelError::Refresh(_) | ChannelError::Login(_),
            ) => StatusCode::BAD_GATEWAY,
            Self::Channel(
                ChannelError::Prepare(_)
                | ChannelError::Decode(_)
                | ChannelError::Observe(_)
                | ChannelError::Unsupported(_),
            ) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SurfaceState(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// OpenAI-style error envelope every host renders identically.
    pub fn body_json(&self) -> serde_json::Value {
        serde_json::json!({ "error": { "message": self.to_string() } })
    }
}

/// Failures from host-provided persistence (credential store, cache).
#[derive(Debug, thiserror::Error)]
#[error("store: {0}")]
pub struct StoreError(pub String);
