use crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;
use crate::openai::create_response::request::OpenAiCreateResponseRequest;
use crate::transform::utils::TransformError;

impl TryFrom<&OpenAiCreateResponseRequest> for OpenAiChatCompletionsRequest {
    type Error = TransformError;

    fn try_from(value: &OpenAiCreateResponseRequest) -> Result<Self, TransformError> {
        OpenAiChatCompletionsRequest::try_from(value.clone())
    }
}
