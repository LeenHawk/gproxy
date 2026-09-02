pub(crate) mod generate_content;
pub(crate) mod imagen;
pub(crate) mod responses;
pub(crate) mod stream;

use crate::TransformError;

pub(super) fn encode(value: &impl serde::Serialize) -> Result<bytes::Bytes, TransformError> {
    Ok(bytes::Bytes::from(serde_json::to_vec(value)?))
}
