use crate::protocol::{gemini, openai};
use crate::transform::generate_content::openai_responses_to_gemini_generate_content;
use crate::transform::{TransformContext, TransformError};

use super::super::openai_compact_to_openai_responses;

/// `compact -> responses -> gemini` request.
pub fn request(
    input: openai::CompactResponseRequestBody,
    ctx: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let responses = openai_compact_to_openai_responses::request(input, ctx)?;
    openai_responses_to_gemini_generate_content::request(responses, ctx)
}
