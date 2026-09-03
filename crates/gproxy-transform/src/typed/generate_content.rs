//! Direct request and response pairs for content-generation protocols.

use gproxy_protocol::{claude, gemini, openai};

use super::RequestContext;
use crate::TransformError;

macro_rules! pair {
    (
        $module:ident,
        request $request_in:ty => $request_out:ty, $request:path,
        response $response_in:ty => $response_out:ty, $response:path
    ) => {
        pub mod $module {
            use super::*;

            pub fn request(
                input: $request_in,
                context: RequestContext<'_>,
            ) -> Result<$request_out, TransformError> {
                $request(input, context.upstream_model, context.stream)
            }

            pub fn response(input: $response_in) -> Result<$response_out, TransformError> {
                $response(input)
            }
        }
    };
}

pair!(
    openai_chat_to_claude_messages,
    request openai::ChatCompletionRequest => claude::CreateMessageRequestBody,
        crate::generate_content::openai_chat_to_claude_messages::request::transform_typed,
    response claude::CreateMessageResponseBody => openai::ChatCompletionResponse,
        crate::generate_content::openai_chat_to_claude_messages::response::transform_typed
);
pair!(
    claude_messages_to_openai_chat,
    request claude::CreateMessageRequestBody => openai::ChatCompletionRequest,
        crate::generate_content::claude_messages_to_openai_chat::request::transform_typed,
    response openai::ChatCompletionResponse => claude::CreateMessageResponseBody,
        crate::generate_content::claude_messages_to_openai_chat::response::transform_typed
);
pair!(
    openai_responses_to_claude_messages,
    request openai::ResponseCreateRequest => claude::CreateMessageRequestBody,
        crate::generate_content::openai_responses_to_claude_messages::request::transform_typed,
    response claude::CreateMessageResponseBody => openai::ResponseObject,
        crate::generate_content::openai_responses_to_claude_messages::response::transform_typed
);
pair!(
    claude_messages_to_openai_responses,
    request claude::CreateMessageRequestBody => openai::ResponseCreateRequest,
        crate::generate_content::claude_messages_to_openai_responses::request::transform_typed,
    response openai::ResponseObject => claude::CreateMessageResponseBody,
        crate::generate_content::claude_messages_to_openai_responses::response::transform_typed
);
pair!(
    openai_chat_to_gemini_generate_content,
    request openai::ChatCompletionRequest => gemini::GenerateContentRequest,
        crate::generate_content::openai_chat_to_gemini_generate_content::request::transform_typed,
    response gemini::GenerateContentResponse => openai::ChatCompletionResponse,
        crate::generate_content::openai_chat_to_gemini_generate_content::response::transform_typed
);
pair!(
    gemini_generate_content_to_openai_chat,
    request gemini::GenerateContentRequest => openai::ChatCompletionRequest,
        crate::generate_content::gemini_generate_content_to_openai_chat::request::transform_typed,
    response openai::ChatCompletionResponse => gemini::GenerateContentResponse,
        crate::generate_content::gemini_generate_content_to_openai_chat::response::transform_typed
);
pair!(
    openai_responses_to_gemini_generate_content,
    request openai::ResponseCreateRequest => gemini::GenerateContentRequest,
        crate::generate_content::openai_responses_to_gemini_generate_content::request::transform_typed,
    response gemini::GenerateContentResponse => openai::ResponseObject,
        crate::generate_content::openai_responses_to_gemini_generate_content::response::transform_typed
);
pair!(
    gemini_generate_content_to_openai_responses,
    request gemini::GenerateContentRequest => openai::ResponseCreateRequest,
        crate::generate_content::gemini_generate_content_to_openai_responses::request::transform_typed,
    response openai::ResponseObject => gemini::GenerateContentResponse,
        crate::generate_content::gemini_generate_content_to_openai_responses::response::transform_typed
);
pair!(
    claude_messages_to_gemini_generate_content,
    request claude::CreateMessageRequestBody => gemini::GenerateContentRequest,
        crate::generate_content::claude_messages_to_gemini_generate_content::request::transform_typed,
    response gemini::GenerateContentResponse => claude::CreateMessageResponseBody,
        crate::generate_content::gemini_generate_content_to_claude_messages::response::transform_typed
);
pair!(
    gemini_generate_content_to_claude_messages,
    request gemini::GenerateContentRequest => claude::CreateMessageRequestBody,
        crate::generate_content::gemini_generate_content_to_claude_messages::request::transform_typed,
    response claude::CreateMessageResponseBody => gemini::GenerateContentResponse,
        crate::generate_content::claude_messages_to_gemini_generate_content::response::transform_typed
);
pair!(
    openai_chat_to_openai_responses,
    request openai::ChatCompletionRequest => openai::ResponseCreateRequest,
        crate::generate_content::openai_chat_to_openai_responses::request::transform_typed,
    response openai::ResponseObject => openai::ChatCompletionResponse,
        crate::generate_content::openai_chat_to_openai_responses::response::transform_typed
);
pair!(
    openai_responses_to_openai_chat,
    request openai::ResponseCreateRequest => openai::ChatCompletionRequest,
        crate::generate_content::openai_responses_to_openai_chat::request::transform_typed,
    response openai::ChatCompletionResponse => openai::ResponseObject,
        crate::generate_content::openai_responses_to_openai_chat::response::transform_typed
);
