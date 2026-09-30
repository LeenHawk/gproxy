use super::EmbeddingBatchOptions;
use crate::{
    codec::{self, CodecErrorKind, CodecLimits},
    transform::{TransformError, TransformErrorKind},
    wire::gemini::embeddings as g,
};

pub(super) fn gemini_batches(
    input: Vec<g::BatchEmbedContentRequest>,
    options: &EmbeddingBatchOptions,
    write_bytes: u64,
) -> Result<Vec<g::BatchEmbedContentsRequestBody>, TransformError> {
    for facts in options.usage_per_call.iter().flatten() {
        if facts.prompt_tokens < 0 || facts.total_tokens < facts.prompt_tokens {
            return Err(TransformError::shape(
                "embedding.usage_per_call",
                "usage must be nonnegative and total must cover prompt tokens",
            ));
        }
    }
    let mut limits = options.codec;
    limits.max_body_bytes = limits.max_body_bytes.min(write_bytes);
    limits.max_value_bytes = limits.max_value_bytes.min(write_bytes);
    limits.max_buffer_bytes = limits.max_buffer_bytes.min(write_bytes);
    let mut batches = Vec::new();
    let mut pending = g::BatchEmbedContentsRequestBody::builder(Vec::new()).build();
    for item in input {
        pending.requests.push(item);
        match codec::encode_json(&pending, limits) {
            Ok(_) => {}
            Err(error) if error.kind() == CodecErrorKind::Limit && pending.requests.len() > 1 => {
                let last = pending.requests.pop().expect("just inserted");
                batches.push(std::mem::replace(
                    &mut pending,
                    g::BatchEmbedContentsRequestBody::builder(vec![last]).build(),
                ));
                encoded(&pending, limits)?;
            }
            Err(error) => return Err(encoding(error)),
        }
    }
    if !pending.requests.is_empty() {
        batches.push(pending);
    }
    if !options.usage_per_call.is_empty() && options.usage_per_call.len() != batches.len() {
        return Err(TransformError::shape(
            "embedding.usage_per_call",
            "usage supplement count must match the planned calls",
        ));
    }
    Ok(batches)
}

fn encoded(
    value: &g::BatchEmbedContentsRequestBody,
    limits: CodecLimits,
) -> Result<(), TransformError> {
    codec::encode_json(value, limits)
        .map(|_| ())
        .map_err(encoding)
}

pub(super) fn encoding(error: codec::CodecError) -> TransformError {
    let kind = if error.kind() == CodecErrorKind::Limit {
        TransformErrorKind::Limit
    } else {
        TransformErrorKind::InvalidResult
    };
    let detail = error.to_string();
    TransformError::with_source(kind, "embedding.encoded_body", detail, error)
}
