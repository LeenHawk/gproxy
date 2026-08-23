use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    if let Ok(list) = serde_json::from_slice::<openai::ModelListResponse>(&body) {
        let mut rest = list.rest;
        let first_id = super::super::common::take(&mut rest, "first_id")?;
        let last_id = super::super::common::take(&mut rest, "last_id")?;
        let has_more = super::super::common::take(&mut rest, "has_more")?;
        let data = list
            .data
            .into_iter()
            .map(super::super::common::openai_to_claude)
            .collect::<Result<Vec<_>, _>>()?;
        let output = claude::ListModelsResponse {
            first_id: first_id.or(data
                .first()
                .map(|model| super::super::common::wire_string(&model.id))
                .transpose()?),
            last_id: last_id.or(data
                .last()
                .map(|model| super::super::common::wire_string(&model.id))
                .transpose()?),
            data,
            has_more,
            rest,
        };
        return Ok(Bytes::from(serde_json::to_vec(&output)?));
    }
    let model: openai::Model = serde_json::from_slice(&body)?;
    Ok(Bytes::from(serde_json::to_vec(
        &super::super::common::openai_to_claude(model)?,
    )?))
}
