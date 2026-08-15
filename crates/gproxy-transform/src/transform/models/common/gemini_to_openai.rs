use crate::protocol::{gemini, openai};
use crate::transform::TransformContext;

use super::{DEFAULT_OPENAI_OWNED_BY, gemini_model_id, i32_to_u64_default, openai_model_object};

pub(in crate::transform::models) fn model(
    input: gemini::Model,
    _: &TransformContext,
) -> openai::Model {
    crate::protocol::wire!(openai::Model {
        id: gemini_model_id(&input).into(),
        created: None,
        max_input_tokens: input.input_token_limit.map(i32_to_u64_default),
        max_output_tokens: input.output_token_limit.map(i32_to_u64_default),
        object: openai_model_object(),
        owned_by: DEFAULT_OPENAI_OWNED_BY.to_owned(),
        extra: Default::default(),
    })
}
