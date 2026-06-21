use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ResponseCreateRequest,
    ctx: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let chat = super::super::openai_responses_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_gemini_generate_content::request(chat, ctx)
}
