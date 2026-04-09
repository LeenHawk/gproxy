use crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;
use crate::openai::create_response::request::OpenAiCreateResponseRequest;
use crate::transform::utils::TransformError;

impl TryFrom<&OpenAiChatCompletionsRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: &OpenAiChatCompletionsRequest) -> Result<Self, TransformError> {
        OpenAiCreateResponseRequest::try_from(value.clone())
    }
}
