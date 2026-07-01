//! OpenAI -> Gemini batch embedding transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: openai::EmbeddingRequest,
    _: &TransformContext,
) -> Result<gemini::BatchEmbedContentsRequest, TransformError> {
    Ok(gemini::BatchEmbedContentsRequest {
        requests: common::openai_to_gemini_requests(input),
        extra: Default::default(),
    })
}

pub fn response(
    input: openai::EmbeddingResponse,
    _: &TransformContext,
) -> Result<gemini::BatchEmbedContentsResponse, TransformError> {
    Ok(gemini::BatchEmbedContentsResponse {
        embeddings: input
            .data
            .into_iter()
            .map(common::openai_to_gemini_embedding)
            .collect(),
        usage_metadata: Some(common::openai_to_gemini_usage(input.usage)),
        extra: Default::default(),
    })
}
