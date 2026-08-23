use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CountTokensResponseBody = serde_json::from_slice(&body)?;
    let output = openai::ResponseInputTokensResponse {
        input_tokens: input.input_tokens.min(u64::from(u32::MAX)) as u32,
        object: openai::ResponseInputTokensObjectType::ResponseInputTokens,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
