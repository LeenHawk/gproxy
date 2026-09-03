use bytes::Bytes;
use gproxy_protocol::claude;

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<claude::ListModelsResponse>(&body) {
        let output = crate::typed::models::openai_to_claude::list_response(list)?;
        return Ok(Bytes::from(serde_json::to_vec(&output)?));
    }
    let model: claude::ModelInfo = serde_json::from_slice(&body)?;
    Ok(Bytes::from(serde_json::to_vec(
        &crate::typed::models::openai_to_claude::get_response(model)?,
    )?))
}
