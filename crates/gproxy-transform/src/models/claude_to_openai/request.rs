use bytes::Bytes;

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if body.is_empty() {
        Ok(body)
    } else {
        Err(TransformError::shape(
            "models request",
            "body must be empty",
        ))
    }
}
