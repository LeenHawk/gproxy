//! Gemini -> Claude get-model transforms.

use crate::protocol::{claude, gemini};
use crate::transform::TransformContext;

use super::super::common;

pub fn request(input: gemini::GetModelRequest, _: &TransformContext) -> claude::RetrieveModelPath {
    claude::RetrieveModelPath {
        model_id: input.name.unwrap_or_default().into(),
    }
}

pub fn response(input: gemini::Model, ctx: &TransformContext) -> claude::ModelInfo {
    common::gemini_to_claude::model(input, ctx)
}
