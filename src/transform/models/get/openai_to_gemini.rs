//! OpenAI -> Gemini get-model transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn response(
    input: openai::Model,
    ctx: &TransformContext,
) -> Result<gemini::Model, TransformError> {
    common::openai_to_gemini::model(input, ctx)
}
