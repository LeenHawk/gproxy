//! Claude -> OpenAI get-model transforms.

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn response(
    input: claude::ModelInfo,
    ctx: &TransformContext,
) -> Result<openai::Model, TransformError> {
    common::claude_to_openai::model(input, ctx)
}
