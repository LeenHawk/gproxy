use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::{DEFAULT_OPENAI_OWNED_BY, openai_model_object, wire_string};

pub(in crate::transform::models) fn model(
    input: claude::ModelInfo,
    _: &TransformContext,
) -> Result<openai::Model, TransformError> {
    Ok(openai::Model {
        id: wire_string(&input.id, "id")?.into(),
        created: None,
        object: openai_model_object(),
        owned_by: DEFAULT_OPENAI_OWNED_BY.to_owned(),
        extra: Default::default(),
    })
}
