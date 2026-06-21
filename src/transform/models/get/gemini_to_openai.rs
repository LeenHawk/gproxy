//! Gemini -> OpenAI get-model transforms.

use crate::protocol::{gemini, openai};
use crate::transform::TransformContext;

use super::super::common;

pub fn response(input: gemini::Model, ctx: &TransformContext) -> openai::Model {
    common::gemini_to_openai::model(input, ctx)
}
