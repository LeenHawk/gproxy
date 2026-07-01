use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: claude::CreateMessageRequestBody,
    ctx: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let chat = super::super::claude_messages_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_openai_responses::request(chat, ctx)
}
