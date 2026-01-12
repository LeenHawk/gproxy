use crate::formats::{gemini, openai};

use super::TransformError;

pub fn request(
    request: openai::chat_completions::CreateChatCompletionRequest,
) -> Result<gemini::generate_content::GenerateContentRequest, TransformError> {
    super::gen_openai_chat_to_gemini_generate::request(request)
}
