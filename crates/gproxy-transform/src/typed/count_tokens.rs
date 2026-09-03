//! Direct Count Tokens protocol pairs.

use gproxy_protocol::{claude, gemini, openai};

use super::RequestContext;
use crate::TransformError;

pub mod openai_to_claude {
    use super::*;

    pub fn request(
        input: openai::ResponseInputTokensRequest,
        context: RequestContext<'_>,
    ) -> Result<claude::CountTokensRequestBody, TransformError> {
        crate::count_tokens::openai_to_claude::request::transform_typed(
            input,
            context.upstream_model,
        )
    }

    pub fn response(input: claude::CountTokensResponseBody) -> openai::ResponseInputTokensResponse {
        crate::count_tokens::openai_to_claude::response::transform_typed(input)
    }
}

pub mod claude_to_openai {
    use super::*;

    pub fn request(
        input: claude::CountTokensRequestBody,
        context: RequestContext<'_>,
    ) -> Result<openai::ResponseInputTokensRequest, TransformError> {
        crate::count_tokens::claude_to_openai::request::transform_typed(
            input,
            context.upstream_model,
        )
    }

    pub fn response(input: openai::ResponseInputTokensResponse) -> claude::CountTokensResponseBody {
        crate::count_tokens::claude_to_openai::response::transform_typed(input)
    }
}

pub mod openai_to_gemini {
    use super::*;

    pub fn request(
        input: openai::ResponseInputTokensRequest,
        context: RequestContext<'_>,
    ) -> Result<gemini::CountTokensRequest, TransformError> {
        crate::count_tokens::gemini::openai_to_gemini(input, context.upstream_model)
    }

    pub fn response(input: gemini::CountTokensResponse) -> openai::ResponseInputTokensResponse {
        crate::count_tokens::gemini::gemini_response_to_openai(input)
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn request(
        input: gemini::CountTokensRequest,
        context: RequestContext<'_>,
    ) -> Result<openai::ResponseInputTokensRequest, TransformError> {
        crate::count_tokens::gemini::gemini_to_openai(input, context.upstream_model)
    }

    pub fn response(input: openai::ResponseInputTokensResponse) -> gemini::CountTokensResponse {
        crate::count_tokens::gemini::openai_response_to_gemini(input)
    }
}

pub mod claude_to_gemini {
    use super::*;

    pub fn request(
        input: claude::CountTokensRequestBody,
        context: RequestContext<'_>,
    ) -> Result<gemini::CountTokensRequest, TransformError> {
        crate::count_tokens::gemini::claude_to_gemini(input, context.upstream_model)
    }

    pub fn response(input: gemini::CountTokensResponse) -> claude::CountTokensResponseBody {
        crate::count_tokens::gemini::gemini_response_to_claude(input)
    }
}

pub mod gemini_to_claude {
    use super::*;

    pub fn request(
        input: gemini::CountTokensRequest,
        context: RequestContext<'_>,
    ) -> Result<claude::CountTokensRequestBody, TransformError> {
        crate::count_tokens::gemini::gemini_to_claude(input, context.upstream_model)
    }

    pub fn response(input: claude::CountTokensResponseBody) -> gemini::CountTokensResponse {
        crate::count_tokens::gemini::claude_response_to_gemini(input)
    }
}
