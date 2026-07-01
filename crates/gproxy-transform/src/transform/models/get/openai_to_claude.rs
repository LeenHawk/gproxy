//! OpenAI -> Claude get-model transforms.

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn response(
    input: openai::Model,
    ctx: &TransformContext,
) -> Result<claude::ModelInfo, TransformError> {
    common::openai_to_claude::model(input, ctx)
}
