use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: claude::CreateMessageResponseBody,
    ctx: &TransformContext,
) -> Result<openai::ResponseObject, TransformError> {
    let chat = super::super::claude_messages_to_openai_chat::response(input, ctx)?;
    super::super::openai_chat_to_openai_responses::response(chat, ctx)
}
