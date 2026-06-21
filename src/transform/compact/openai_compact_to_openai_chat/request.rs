use crate::protocol::openai;
use crate::transform::generate_content::openai_responses_to_openai_chat;
use crate::transform::{TransformContext, TransformError};

use super::super::openai_compact_to_openai_responses;

/// `compact -> responses -> chat` request.
pub fn request(
    input: openai::CompactResponseRequestBody,
    ctx: &TransformContext,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let responses = openai_compact_to_openai_responses::request(input, ctx)?;
    openai_responses_to_openai_chat::request(responses, ctx)
}
