use super::{EmbeddingBatchOptions, packing};
use crate::{
    codec::{self, CodecErrorKind},
    transform::TransformError,
    wire::openai::embeddings as o,
};

pub(super) fn batches(
    input: Vec<o::CreateEmbeddingRequestBody>,
    options: &EmbeddingBatchOptions,
    write_bytes: u64,
) -> Result<Vec<o::CreateEmbeddingRequestBody>, TransformError> {
    if !options.usage_per_call.is_empty() {
        return Err(TransformError::shape(
            "embedding.usage_per_call",
            "OpenAI returns required actual usage; supplements apply only to Gemini target calls",
        ));
    }
    let mut limits = options.codec;
    limits.max_body_bytes = limits.max_body_bytes.min(write_bytes);
    limits.max_value_bytes = limits.max_value_bytes.min(write_bytes);
    limits.max_buffer_bytes = limits.max_buffer_bytes.min(write_bytes);
    let mut out: Vec<o::CreateEmbeddingRequestBody> = Vec::new();
    for mut next in input {
        let o::EmbeddingInput::Text(text) = next.input else {
            return Err(TransformError::shape(
                "embedding.input",
                "expected a prepared single-text request",
            ));
        };
        next.input = o::EmbeddingInput::Texts(vec![text.clone()]);
        codec::encode_json(&next, limits).map_err(packing::encoding)?;
        if let Some(previous) = out.last_mut()
            && previous.model == next.model
            && previous.dimensions == next.dimensions
            && previous.encoding_format == next.encoding_format
            && previous.user == next.user
            && let o::EmbeddingInput::Texts(texts) = &mut previous.input
        {
            texts.push(text);
            match codec::encode_json(previous, limits) {
                Ok(_) => continue,
                Err(error) if error.kind() == CodecErrorKind::Limit => {
                    let o::EmbeddingInput::Texts(texts) = &mut previous.input else {
                        unreachable!("prepared batch")
                    };
                    texts.pop();
                }
                Err(error) => return Err(packing::encoding(error)),
            }
        }

        out.push(next);
    }
    Ok(out)
}
