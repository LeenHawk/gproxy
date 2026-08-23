use gproxy_protocol::claude;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CountTokensRequestBody = serde_json::from_slice(&body)?;
    let output =
        crate::generate_content::claude_messages_to_openai_responses::request::count_tokens(
            input, model,
        )?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
