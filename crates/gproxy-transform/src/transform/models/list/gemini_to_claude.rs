//! Gemini -> Claude list-models transforms.

use crate::protocol::{claude, gemini};
use crate::transform::TransformContext;

use super::super::common::{self, i32_to_u64_default};

pub fn request(input: gemini::ListModelsRequest, _: &TransformContext) -> claude::ListModelsQuery {
    claude::ListModelsQuery {
        after_id: input.page_token,
        before_id: None,
        limit: input.page_size.map(i32_to_u64_default),
        extra: Default::default(),
    }
}

pub fn response(
    input: gemini::ListModelsResponse,
    ctx: &TransformContext,
) -> claude::ListModelsResponse {
    let has_more = input.next_page_token.is_some();
    let data = input
        .models
        .into_iter()
        .map(|model| common::gemini_to_claude::model(model, ctx))
        .collect::<Vec<_>>();

    let first_id = data
        .first()
        .map(common::claude_model_id)
        .unwrap_or_default();
    let last_id = input
        .next_page_token
        .or_else(|| data.last().map(common::claude_model_id))
        .unwrap_or_default();

    claude::ListModelsResponse {
        data,
        first_id,
        has_more,
        last_id,
        extra: Default::default(),
    }
}
