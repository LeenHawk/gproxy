use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ResponseCreateRequest,
    ctx: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let chat = super::super::openai_responses_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_claude_messages::request(chat, ctx)
}
