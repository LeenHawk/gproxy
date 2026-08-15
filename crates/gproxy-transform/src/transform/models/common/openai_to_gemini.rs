use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::{u64_to_i32_default, wire_string};

pub(in crate::transform::models) fn model(
    input: openai::Model,
    _: &TransformContext,
) -> Result<gemini::Model, TransformError> {
    let id = wire_string(&input.id, "id")?;

    Ok(crate::protocol::wire!(gemini::Model {
        name: Some(id.clone()),
        base_model_id: Some(id.clone()),
        version: None,
        display_name: Some(id),
        description: None,
        input_token_limit: input.max_input_tokens.map(u64_to_i32_default),
        output_token_limit: input.max_output_tokens.map(u64_to_i32_default),
        supported_generation_methods: Vec::new(),
        supported_actions: Vec::new(),
        thinking: None,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
        extra: Default::default(),
    }))
}
