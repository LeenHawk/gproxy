use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseInputTokensResponse = serde_json::from_slice(&body)?;
    let output = claude::CountTokensResponseBody {
        input_tokens: u64::from(input.input_tokens),
        context_management: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
