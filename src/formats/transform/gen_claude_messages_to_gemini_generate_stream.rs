use crate::formats::{claude, gemini};

use super::TransformError;

pub fn request(
    request: claude::messages::MessageCreateRequest,
) -> Result<gemini::generate_content::GenerateContentRequest, TransformError> {
    super::gen_claude_messages_to_gemini_generate::request(request)
}
