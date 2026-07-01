use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::{DEFAULT_CREATED_AT, claude_model_object, default_claude_capabilities, wire_string};

pub(in crate::transform::models) fn model(
    input: openai::Model,
    _: &TransformContext,
) -> Result<claude::ModelInfo, TransformError> {
    let id = wire_string(&input.id, "id")?;

    Ok(claude::ModelInfo {
        id: id.clone().into(),
        type_: claude_model_object(),
        created_at: DEFAULT_CREATED_AT.to_owned(),
        display_name: id,
        max_input_tokens: 0,
        max_tokens: 0,
        capabilities: default_claude_capabilities(),
        extra: Default::default(),
    })
}
