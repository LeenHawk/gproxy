//! OpenAI -> Gemini single embedding transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: openai::EmbeddingRequest,
    _: &TransformContext,
) -> Result<gemini::EmbedContentRequest, TransformError> {
    Ok(common::openai_to_gemini_requests(input)
        .pop()
        .unwrap_or_default())
}

pub fn response(
    input: openai::EmbeddingResponse,
    _: &TransformContext,
) -> Result<gemini::EmbedContentResponse, TransformError> {
    Ok(gemini::EmbedContentResponse {
        embedding: input
            .data
            .into_iter()
            .map(common::openai_to_gemini_embedding)
            .next(),
        usage_metadata: Some(common::openai_to_gemini_usage(input.usage)),
        extra: Default::default(),
    })
}
