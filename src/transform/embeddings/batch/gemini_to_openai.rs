//! Gemini -> OpenAI batch embedding transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: gemini::BatchEmbedContentsRequest,
    _: &TransformContext,
) -> Result<openai::EmbeddingRequest, TransformError> {
    let requests = input.requests;
    let mut inputs = Vec::with_capacity(requests.len());
    let mut model = None;
    let mut dimensions = None;

    for request in requests {
        let converted = common::gemini_request_parts(request);
        inputs.push(converted.text);
        common::merge_model(&mut model, converted.model);
        common::merge_dimensions(&mut dimensions, converted.dimensions);
    }

    Ok(openai::EmbeddingRequest {
        input: common::strings_to_openai_input(inputs),
        model: model
            .unwrap_or_else(|| common::DEFAULT_OPENAI_EMBEDDING_MODEL.to_owned())
            .into(),
        dimensions,
        encoding_format: Some(openai::EmbeddingEncodingFormat::Float),
        user: None,
        extra: Default::default(),
    })
}

pub fn response(
    input: gemini::BatchEmbedContentsResponse,
    _: &TransformContext,
) -> Result<openai::EmbeddingResponse, TransformError> {
    Ok(openai::EmbeddingResponse {
        data: input
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| common::gemini_to_openai_embedding(embedding, index))
            .collect(),
        model: common::DEFAULT_OPENAI_EMBEDDING_MODEL.to_owned().into(),
        object: openai::ListObjectType::List,
        usage: common::gemini_to_openai_usage(input.usage_metadata),
        extra: Default::default(),
    })
}
