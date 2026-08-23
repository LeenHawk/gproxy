use gproxy_protocol::OperationKey;

#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    #[error("unsupported transform pair: {source_key:?} -> {target_key:?}")]
    UnsupportedPair {
        source_key: OperationKey,
        target_key: OperationKey,
    },
    #[error("invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid {wire} wire shape: {message}")]
    InvalidShape { wire: &'static str, message: String },
    #[error("unsupported {wire} field or event: {name}")]
    Unsupported { wire: &'static str, name: String },
    #[error("stream ended with an incomplete SSE frame")]
    IncompleteStream,
}

impl TransformError {
    pub(crate) fn shape(wire: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidShape {
            wire,
            message: message.into(),
        }
    }

    pub(crate) fn unsupported(wire: &'static str, name: impl Into<String>) -> Self {
        Self::Unsupported {
            wire,
            name: name.into(),
        }
    }
}
