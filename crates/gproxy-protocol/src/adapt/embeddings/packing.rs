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
    if options.max_calls == 0 || options.max_items_per_call == 0 {
        return Err(TransformError::shape(
            "embedding.batch_limits",
            "call and item limits must be positive",
        ));
    }
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
        if pending.requests.len() == options.max_items_per_call {
            push(
                &mut batches,
                std::mem::replace(
                    &mut pending,
                    g::BatchEmbedContentsRequestBody::builder(Vec::new()).build(),
                ),
                options.max_calls,
            )?;
        }
        pending.requests.push(item);
        match codec::encode_json(&pending, limits) {
            Ok(_) => {}
            Err(error) if error.kind() == CodecErrorKind::Limit && pending.requests.len() > 1 => {
                let last = pending.requests.pop().expect("just inserted");
                push(
                    &mut batches,
                    std::mem::replace(
                        &mut pending,
                        g::BatchEmbedContentsRequestBody::builder(vec![last]).build(),
                    ),
                    options.max_calls,
                )?;
                encoded(&pending, limits)?;
            }
            Err(error) => return Err(encoding(error)),
        }
    }
    if !pending.requests.is_empty() {
        push(&mut batches, pending, options.max_calls)?;
    }
    if !options.usage_per_call.is_empty() && options.usage_per_call.len() != batches.len() {
        return Err(TransformError::shape(
            "embedding.usage_per_call",
            "usage supplement count must match the planned calls",
        ));
    }
    Ok(batches)
}

fn push(
    batches: &mut Vec<g::BatchEmbedContentsRequestBody>,
    batch: g::BatchEmbedContentsRequestBody,
    max_calls: usize,
) -> Result<(), TransformError> {
    if batches.len() == max_calls {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "embedding.max_calls",
            "batch requires more upstream calls than allowed",
        ));
    }
    batches.push(batch);
    Ok(())
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
