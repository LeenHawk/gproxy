use std::{error::Error, fmt};

use crate::protocol::OperationKey;

/// Errors returned by provider-to-provider transforms.
#[derive(Debug)]
pub enum TransformError {
    UnsupportedPair {
        source: OperationKey,
        target: OperationKey,
    },
    UnsupportedField {
        field: &'static str,
        reason: &'static str,
    },
    LossyField {
        field: &'static str,
        reason: &'static str,
    },
    InvalidInput {
        reason: String,
    },
    Serialization {
        reason: String,
    },
}

impl TransformError {
    pub const fn unsupported_pair(source: OperationKey, target: OperationKey) -> Self {
        Self::UnsupportedPair { source, target }
    }
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPair { source, target } => {
                write!(f, "unsupported transform pair: {source:?} -> {target:?}")
            }
            Self::UnsupportedField { field, reason } => {
                write!(f, "unsupported field `{field}`: {reason}")
            }
            Self::LossyField { field, reason } => {
                write!(f, "lossy field `{field}`: {reason}")
            }
            Self::InvalidInput { reason } => write!(f, "invalid transform input: {reason}"),
            Self::Serialization { reason } => {
                write!(f, "transform serialization failed: {reason}")
            }
        }
    }
}

impl Error for TransformError {}
