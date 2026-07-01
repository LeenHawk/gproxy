//! OpenAI -> Gemini list-models transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(_: (), _: &TransformContext) -> gemini::ListModelsRequest {
    gemini::ListModelsRequest::default()
}

pub fn response(
    input: openai::ModelListResponse,
    ctx: &TransformContext,
) -> Result<gemini::ListModelsResponse, TransformError> {
    Ok(gemini::ListModelsResponse {
        models: input
            .data
            .into_iter()
            .map(|model| common::openai_to_gemini::model(model, ctx))
            .collect::<Result<Vec<_>, _>>()?,
        next_page_token: None,
        extra: Default::default(),
    })
}
