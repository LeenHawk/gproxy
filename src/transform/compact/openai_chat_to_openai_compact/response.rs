use crate::protocol::openai;
use crate::transform::generate_content::openai_chat_to_openai_responses;
use crate::transform::{TransformContext, TransformError};

use super::super::openai_responses_to_openai_compact;

/// `chat -> responses -> compact` response.
pub fn response(
    input: openai::ChatCompletionResponse,
    ctx: &TransformContext,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let responses = openai_chat_to_openai_responses::response(input, ctx)?;
    Ok(openai_responses_to_openai_compact::response(responses, ctx))
}
