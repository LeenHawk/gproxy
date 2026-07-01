use crate::protocol::{gemini, openai};
use crate::transform::TransformContext;

use super::{DEFAULT_OPENAI_OWNED_BY, gemini_model_id, openai_model_object};

pub(in crate::transform::models) fn model(
    input: gemini::Model,
    _: &TransformContext,
) -> openai::Model {
    openai::Model {
        id: gemini_model_id(&input).into(),
        created: None,
        object: openai_model_object(),
        owned_by: DEFAULT_OPENAI_OWNED_BY.to_owned(),
        extra: Default::default(),
    }
}
