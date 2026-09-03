use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<openai::ModelListResponse>(&body) {
        let output = crate::typed::models::claude_to_openai::list_response(list)?;
        return Ok(Bytes::from(serde_json::to_vec(&output)?));
    }
    let model: openai::Model = serde_json::from_slice(&body)?;
    Ok(Bytes::from(serde_json::to_vec(
        &crate::typed::models::claude_to_openai::get_response(model)?,
    )?))
}
