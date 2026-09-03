use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<claude::ListModelsResponse>(&body) {
        let output = openai::ModelListResponse {
            data: list
                .data
                .into_iter()
                .map(super::super::common::claude_to_openai)
                .collect::<Result<_, _>>()?,
            object: openai::ListObjectType::List,
            rest: Default::default(),
        };
        return Ok(Bytes::from(serde_json::to_vec(&output)?));
    }
    let model: claude::ModelInfo = serde_json::from_slice(&body)?;
    Ok(Bytes::from(serde_json::to_vec(
        &super::super::common::claude_to_openai(model)?,
    )?))
}
