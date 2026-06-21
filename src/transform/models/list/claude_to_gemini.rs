//! Claude -> Gemini list-models transforms.

use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common::{self, u64_to_i32_default};

pub fn request(input: claude::ListModelsQuery, _: &TransformContext) -> gemini::ListModelsRequest {
    gemini::ListModelsRequest {
        page_size: input.limit.map(u64_to_i32_default),
        page_token: input.after_id,
        extra: Default::default(),
    }
}

pub fn response(
    input: claude::ListModelsResponse,
    ctx: &TransformContext,
) -> Result<gemini::ListModelsResponse, TransformError> {
    Ok(gemini::ListModelsResponse {
        models: input
            .data
            .into_iter()
            .map(|model| common::claude_to_gemini::model(model, ctx))
            .collect::<Result<Vec<_>, _>>()?,
        next_page_token: if input.has_more {
            Some(input.last_id)
        } else {
            None
        },
        extra: Default::default(),
    })
}
