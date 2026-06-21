use crate::transform::TransformError;

pub fn unsupported_field<T>(
    field: &'static str,
    reason: &'static str,
) -> Result<T, TransformError> {
    Err(TransformError::UnsupportedField { field, reason })
}

pub fn lossy_field<T>(field: &'static str, reason: &'static str) -> Result<T, TransformError> {
    Err(TransformError::LossyField { field, reason })
}

pub fn serialization_error(reason: impl Into<String>) -> TransformError {
    TransformError::Serialization {
        reason: reason.into(),
    }
}
