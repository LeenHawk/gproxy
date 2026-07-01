//! OpenAI -> Claude list-models transforms.

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common::{self, wire_string};

pub fn request(_: (), _: &TransformContext) -> claude::ListModelsQuery {
    claude::ListModelsQuery {
        after_id: None,
        before_id: None,
        limit: None,
        extra: Default::default(),
    }
}

pub fn response(
    input: openai::ModelListResponse,
    ctx: &TransformContext,
) -> Result<claude::ListModelsResponse, TransformError> {
    let data = input
        .data
        .into_iter()
        .map(|model| common::openai_to_claude::model(model, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    let first_id = data
        .first()
        .map(|model| wire_string(&model.id, "id"))
        .transpose()?
        .unwrap_or_default();
    let last_id = data
        .last()
        .map(|model| wire_string(&model.id, "id"))
        .transpose()?
        .unwrap_or_default();

    Ok(claude::ListModelsResponse {
        data,
        first_id,
        has_more: false,
        last_id,
        extra: Default::default(),
    })
}
