use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: gemini::GenerateContentRequest,
    ctx: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let chat = super::super::gemini_generate_content_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_openai_responses::request(chat, ctx)
}
