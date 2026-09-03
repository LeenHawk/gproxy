use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseInputTokensResponse = serde_json::from_slice(&body)?;
    let output = transform_typed(input);
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ResponseInputTokensResponse,
) -> claude::CountTokensResponseBody {
    crate::wire!(claude::CountTokensResponseBody {
        input_tokens: u64::from(input.input_tokens),
        context_management: None,
        rest: Default::default(),
    })
}
