use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseInputTokensRequest = serde_json::from_slice(&body)?;
    let output =
        crate::generate_content::openai_responses_to_claude_messages::request::count_tokens(
            input, model,
        )?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
