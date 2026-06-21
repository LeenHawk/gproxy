//! Gemini -> OpenAI single embedding transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: gemini::EmbedContentRequest,
    _: &TransformContext,
) -> Result<openai::EmbeddingRequest, TransformError> {
    let converted = common::gemini_request_parts(input);
    Ok(openai::EmbeddingRequest {
        input: openai::EmbeddingInput::Text(converted.text),
        model: converted
            .model
            .unwrap_or_else(|| common::DEFAULT_OPENAI_EMBEDDING_MODEL.to_owned())
            .into(),
        dimensions: converted.dimensions,
        encoding_format: Some(openai::EmbeddingEncodingFormat::Float),
        user: None,
        extra: Default::default(),
    })
}

pub fn response(
    input: gemini::EmbedContentResponse,
    ctx: &TransformContext,
) -> Result<openai::EmbeddingResponse, TransformError> {
    super::super::batch::gemini_to_openai::response(
        gemini::BatchEmbedContentsResponse {
            embeddings: input.embedding.into_iter().collect(),
            usage_metadata: input.usage_metadata,
            extra: Default::default(),
        },
        ctx,
    )
}
