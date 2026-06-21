use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: gemini::GenerateContentResponse,
    ctx: &TransformContext,
) -> Result<openai::ResponseObject, TransformError> {
    let chat = super::super::gemini_generate_content_to_openai_chat::response(input, ctx)?;
    super::super::openai_chat_to_openai_responses::response(chat, ctx)
}
