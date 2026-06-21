use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::wire_string;

pub(in crate::transform::models) fn model(
    input: openai::Model,
    _: &TransformContext,
) -> Result<gemini::Model, TransformError> {
    let id = wire_string(&input.id, "id")?;

    Ok(gemini::Model {
        name: Some(id.clone()),
        base_model_id: Some(id.clone()),
        version: None,
        display_name: Some(id),
        description: None,
        input_token_limit: None,
        output_token_limit: None,
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
