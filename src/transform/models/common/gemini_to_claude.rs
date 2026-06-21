use crate::protocol::{claude, gemini};
use crate::transform::TransformContext;

use super::{
    DEFAULT_CREATED_AT, claude_model_object, default_claude_capabilities, gemini_model_id,
    i32_to_u64_default,
};

pub(in crate::transform::models) fn model(
    input: gemini::Model,
    _: &TransformContext,
) -> claude::ModelInfo {
    let id = gemini_model_id(&input);

    claude::ModelInfo {
        id: id.clone().into(),
        type_: claude_model_object(),
        created_at: DEFAULT_CREATED_AT.to_owned(),
        display_name: input.display_name.unwrap_or(id),
        max_input_tokens: input
            .input_token_limit
            .map(i32_to_u64_default)
            .unwrap_or_default(),
        max_tokens: input
            .output_token_limit
            .map(i32_to_u64_default)
            .unwrap_or_default(),
        capabilities: default_claude_capabilities(),
        extra: Default::default(),
    }
}
