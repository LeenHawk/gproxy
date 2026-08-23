use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<claude::ListModelsResponse>(&body) {
        let mut rest = list.rest;
        super::super::common::preserve(&mut rest, "first_id", &list.first_id)?;
        super::super::common::preserve(&mut rest, "last_id", &list.last_id)?;
        super::super::common::preserve(&mut rest, "has_more", &list.has_more)?;
        let output = openai::ModelListResponse {
            data: list
                .data
                .into_iter()
                .map(super::super::common::claude_to_openai)
                .collect::<Result<_, _>>()?,
            object: openai::ListObjectType::List,
            rest,
        };
        return Ok(Bytes::from(serde_json::to_vec(&output)?));
    }
    let model: claude::ModelInfo = serde_json::from_slice(&body)?;
    Ok(Bytes::from(serde_json::to_vec(
        &super::super::common::claude_to_openai(model)?,
    )?))
}
