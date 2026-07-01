use crate::protocol::{gemini, openai};
use crate::transform::generate_content::gemini_generate_content_to_openai_responses;
use crate::transform::{TransformContext, TransformError};

use super::super::openai_responses_to_openai_compact;

/// `gemini -> responses -> compact` response.
pub fn response(
    input: gemini::GenerateContentResponse,
    ctx: &TransformContext,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let responses = gemini_generate_content_to_openai_responses::response(input, ctx)?;
    Ok(openai_responses_to_openai_compact::response(responses, ctx))
}
