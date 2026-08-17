use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::{DEFAULT_OPENAI_OWNED_BY, openai_model_object, wire_string};

pub(in crate::transform::models) fn model(
    input: claude::ModelInfo,
    _: &TransformContext,
) -> Result<openai::Model, TransformError> {
    Ok(crate::protocol::wire!(openai::Model {
        id: wire_string(&input.id, "id")?.into(),
        created: None,
        display_name: Some(input.display_name),
        context_window: input.max_input_tokens,
        max_output_tokens: input.max_tokens,
        thinking_supported: input
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.thinking.supported),
        object: openai_model_object(),
        owned_by: DEFAULT_OPENAI_OWNED_BY.to_owned(),
        extra: Default::default(),
    }))
}
