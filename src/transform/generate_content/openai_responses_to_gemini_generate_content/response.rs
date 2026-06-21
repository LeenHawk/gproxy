use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: openai::ResponseObject,
    ctx: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    let chat = super::super::openai_responses_to_openai_chat::response(input, ctx)?;
    super::super::openai_chat_to_gemini_generate_content::response(chat, ctx)
}
