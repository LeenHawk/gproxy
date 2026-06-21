use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: openai::ResponseObject,
    ctx: &TransformContext,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let chat = super::super::openai_responses_to_openai_chat::response(input, ctx)?;
    super::super::openai_chat_to_claude_messages::response(chat, ctx)
}
