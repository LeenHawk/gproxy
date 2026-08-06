use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::{DEFAULT_CREATED_AT, claude_model_object, wire_string};

pub(in crate::transform::models) fn model(
    input: openai::Model,
    _: &TransformContext,
) -> Result<claude::ModelInfo, TransformError> {
    let id = wire_string(&input.id, "id")?;

    Ok(crate::protocol::wire!(claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: None,
        type_: claude_model_object(),
        created_at: DEFAULT_CREATED_AT.to_owned(),
        display_name: id,
        max_input_tokens: None,
        max_tokens: None,
        capabilities: None,
        extra: Default::default(),
    }))
}
