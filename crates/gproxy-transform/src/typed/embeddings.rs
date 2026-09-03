//! Typed OpenAI and Gemini embedding pairs.

use gproxy_protocol::gemini;
use gproxy_protocol::openai::embeddings as openai;

use super::RequestContext;
use crate::TransformError;

pub mod openai_to_gemini {
    use super::*;

    pub fn request(
        input: openai::CreateEmbeddingRequest,
        context: RequestContext<'_>,
    ) -> Result<gemini::EmbedContentRequest, TransformError> {
        crate::embeddings::openai_to_gemini_single_typed(input, context.upstream_model)
    }

    pub fn batch_request(
        input: openai::CreateEmbeddingRequest,
        context: RequestContext<'_>,
    ) -> Result<gemini::BatchEmbedContentsRequest, TransformError> {
        crate::embeddings::openai_to_gemini_batch_typed(input, context.upstream_model)
    }

    pub fn response(input: gemini::EmbedContentResponse) -> openai::CreateEmbeddingResponse {
        crate::embeddings::gemini_single_response_to_openai_typed(input)
    }

    pub fn batch_response(
        input: gemini::BatchEmbedContentsResponse,
    ) -> openai::CreateEmbeddingResponse {
        crate::embeddings::gemini_batch_response_to_openai_typed(input)
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn request(
        input: gemini::EmbedContentRequest,
        context: RequestContext<'_>,
    ) -> Result<openai::CreateEmbeddingRequest, TransformError> {
        crate::embeddings::gemini_single_to_openai_typed(input, context.upstream_model)
    }

    pub fn batch_request(
        input: gemini::BatchEmbedContentsRequest,
        context: RequestContext<'_>,
    ) -> Result<openai::CreateEmbeddingRequest, TransformError> {
        crate::embeddings::gemini_batch_to_openai_typed(input, context.upstream_model)
    }

    pub fn response(
        input: openai::CreateEmbeddingResponse,
    ) -> Result<gemini::EmbedContentResponse, TransformError> {
        crate::embeddings::openai_response_to_gemini_single_typed(input)
    }

    pub fn batch_response(
        input: openai::CreateEmbeddingResponse,
    ) -> Result<gemini::BatchEmbedContentsResponse, TransformError> {
        crate::embeddings::openai_response_to_gemini_batch_typed(input)
    }
}
