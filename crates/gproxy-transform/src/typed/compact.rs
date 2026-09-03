//! Direct pairs between OpenAI Compact and content-generation protocols.

use gproxy_protocol::{claude, gemini, openai};

use super::RequestContext;
use crate::TransformError;

pub mod openai_compact_to_openai_responses {
    use super::*;

    pub fn request(
        input: openai::CompactResponseRequestBody,
        context: RequestContext<'_>,
    ) -> openai::ResponseCreateRequest {
        crate::compact::other::compact_to_responses_typed(input, context.upstream_model)
    }

    pub fn response(
        input: openai::ResponseObject,
    ) -> Result<openai::CompactedResponseObject, TransformError> {
        crate::compact::openai_to_claude_messages::response::from_responses_typed(input)
    }
}

pub mod openai_responses_to_openai_compact {
    use super::*;

    pub fn request(input: openai::ResponseCreateRequest) -> openai::CompactResponseRequestBody {
        crate::compact::other::responses_to_compact_request_typed(input)
    }

    pub fn response(
        input: openai::CompactedResponseObject,
    ) -> Result<openai::ResponseObject, TransformError> {
        crate::compact::other::compact_object_to_responses_typed(input)
    }
}

pub mod openai_compact_to_openai_chat {
    use super::*;

    pub fn request(
        input: openai::CompactResponseRequestBody,
        context: RequestContext<'_>,
    ) -> Result<openai::ChatCompletionRequest, TransformError> {
        let responses =
            crate::compact::other::compact_to_responses_typed(input, context.upstream_model);
        crate::generate_content::openai_responses_to_openai_chat::request::transform_typed(
            responses,
            context.upstream_model,
            context.stream,
        )
    }

    pub fn response(
        input: openai::ChatCompletionResponse,
    ) -> Result<openai::CompactedResponseObject, TransformError> {
        let responses =
            crate::generate_content::openai_responses_to_openai_chat::response::transform_typed(
                input,
            )?;
        crate::compact::openai_to_claude_messages::response::from_responses_typed(responses)
    }
}

pub mod openai_chat_to_openai_compact {
    use super::*;

    pub fn request(
        input: openai::ChatCompletionRequest,
        context: RequestContext<'_>,
    ) -> Result<openai::CompactResponseRequestBody, TransformError> {
        let responses =
            crate::generate_content::openai_chat_to_openai_responses::request::transform_typed(
                input,
                context.upstream_model,
                false,
            )?;
        Ok(crate::compact::other::responses_to_compact_request_typed(
            responses,
        ))
    }

    pub fn response(
        input: openai::CompactedResponseObject,
    ) -> Result<openai::ChatCompletionResponse, TransformError> {
        let responses = crate::compact::other::compact_object_to_responses_typed(input)?;
        crate::generate_content::openai_chat_to_openai_responses::response::transform_typed(
            responses,
        )
    }
}

pub mod openai_compact_to_claude_messages {
    use super::*;

    pub fn request(
        input: openai::CompactResponseRequestBody,
        context: RequestContext<'_>,
    ) -> Result<claude::CreateMessageRequestBody, TransformError> {
        crate::compact::openai_to_claude_messages::request::transform_typed(
            input,
            context.upstream_model,
        )
    }

    pub fn response(
        input: claude::CreateMessageResponseBody,
    ) -> Result<openai::CompactedResponseObject, TransformError> {
        crate::compact::openai_to_claude_messages::response::transform_typed(input)
    }
}

pub mod claude_messages_to_openai_compact {
    use super::*;

    pub fn request(
        input: claude::CreateMessageRequestBody,
        context: RequestContext<'_>,
    ) -> Result<openai::CompactResponseRequestBody, TransformError> {
        let responses =
            crate::generate_content::claude_messages_to_openai_responses::request::transform_typed(
                input,
                context.upstream_model,
                false,
            )?;
        Ok(crate::compact::other::responses_to_compact_request_typed(
            responses,
        ))
    }

    pub fn response(
        input: openai::CompactedResponseObject,
    ) -> Result<claude::CreateMessageResponseBody, TransformError> {
        let responses = crate::compact::other::compact_object_to_responses_typed(input)?;
        crate::generate_content::claude_messages_to_openai_responses::response::transform_typed(
            responses,
        )
    }
}

pub mod openai_compact_to_gemini_generate_content {
    use super::*;

    pub fn request(
        input: openai::CompactResponseRequestBody,
        context: RequestContext<'_>,
    ) -> Result<gemini::GenerateContentRequest, TransformError> {
        let responses =
            crate::compact::other::compact_to_responses_typed(input, context.upstream_model);
        crate::generate_content::openai_responses_to_gemini_generate_content::request::transform_typed(
            responses,
            context.upstream_model,
            context.stream,
        )
    }

    pub fn response(
        input: gemini::GenerateContentResponse,
    ) -> Result<openai::CompactedResponseObject, TransformError> {
        let responses = crate::generate_content::openai_responses_to_gemini_generate_content::response::transform_typed(input)?;
        crate::compact::openai_to_claude_messages::response::from_responses_typed(responses)
    }
}

pub mod gemini_generate_content_to_openai_compact {
    use super::*;

    pub fn request(
        input: gemini::GenerateContentRequest,
        context: RequestContext<'_>,
    ) -> Result<openai::CompactResponseRequestBody, TransformError> {
        let responses = crate::generate_content::gemini_generate_content_to_openai_responses::request::transform_typed(
            input,
            context.upstream_model,
            false,
        )?;
        Ok(crate::compact::other::responses_to_compact_request_typed(
            responses,
        ))
    }

    pub fn response(
        input: openai::CompactedResponseObject,
    ) -> Result<gemini::GenerateContentResponse, TransformError> {
        let responses = crate::compact::other::compact_object_to_responses_typed(input)?;
        crate::generate_content::gemini_generate_content_to_openai_responses::response::transform_typed(
            responses,
        )
    }
}
