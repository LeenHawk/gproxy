use bytes::Bytes;

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    let _ = body;
    Ok(Bytes::new())
}
