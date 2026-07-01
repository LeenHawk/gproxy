use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::{u64_to_i32_default, wire_string};

pub(in crate::transform::models) fn model(
    input: claude::ModelInfo,
    _: &TransformContext,
) -> Result<gemini::Model, TransformError> {
    let id = wire_string(&input.id, "id")?;

    Ok(gemini::Model {
        name: Some(id.clone()),
        base_model_id: Some(id),
        version: None,
        display_name: Some(input.display_name),
        description: None,
        input_token_limit: Some(u64_to_i32_default(input.max_input_tokens)),
        output_token_limit: Some(u64_to_i32_default(input.max_tokens)),
        supported_generation_methods: Vec::new(),
        supported_actions: Vec::new(),
        thinking: None,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        extra: Default::default(),
    })
}
