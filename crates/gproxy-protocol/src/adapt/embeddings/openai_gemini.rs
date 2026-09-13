use super::{
    EmbeddingBatchError, EmbeddingBatchOptions, EmbeddingBatchResult, EmbeddingFailure, at, packing,
};
use crate::{
    WireRequest,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec,
    transform::{Converted, TransformError, TransformErrorKind, embeddings},
    wire::{DeclaredFields, gemini::embeddings as g, openai::embeddings as o},
};

/// Convert an OpenAI embedding batch and call Gemini batchEmbedContents in
/// source order. `template` is the selected target HTTP endpoint, never the
/// original client request; its body is replaced and authentication is host-owned.
pub async fn openai_to_gemini_batch<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: o::CreateEmbeddingRequestBody,
    target_model: &str,
    options: EmbeddingBatchOptions,
) -> Result<EmbeddingBatchResult<o::CreateEmbeddingResponseBody>, EmbeddingBatchError> {
    let prepared = embeddings::openai_to_gemini_batch(input, target_model)?;
    let mut report = prepared.report;
    let context = prepared.value.response;
    let batches = packing::gemini_batches(
        prepared.value.body.requests,
        &options,
        upstream.limits().write_bytes,
    )?;
    let WireRequest {
        method,
        path,
        query,
        headers,
        body: (),
    } = template;
    let mut data = Vec::new();
    let mut usage = o::EmbeddingUsage::builder(0, 0).build();
    let mut completed = 0;
    for (index, batch) in batches.into_iter().enumerate() {
        let expected = batch.requests.len();
        let request = WireRequest {
            method: method.clone(),
            path: path.clone(),
            query: query.clone(),
            headers: headers.clone(),
            body: batch,
        };
        let result: JsonInvocation<g::BatchEmbedContentsResponseBody> =
            invoke_json(upstream, target, request, options.codec)
                .await
                .map_err(|error| at(error, completed, data.len()))?;
        let response = match result {
            JsonInvocation::Success(response) => {
                completed += 1;
                response.body
            }
            JsonInvocation::Rejected(response) => {
                return Err(EmbeddingBatchError {
                    completed_calls: completed,
                    completed_items: data.len(),
                    failure: EmbeddingFailure::Rejected(Box::new(response)),
                });
            }
        };
        let mut part_context = context.clone();
        part_context.expected_count = expected;
        let converted = part_context
            .finish_batch(
                response,
                options.usage_per_call.get(index).copied().flatten(),
            )
            .map_err(|error| at(error, completed, data.len()))?;
        // Consume the response before any same-dialect field reuse.
        let part = converted.value.into_declared();
        report.diagnostics.extend(converted.report.diagnostics);
        let before = data.len();
        for mut item in part.data {
            item.index = i64::try_from(data.len()).map_err(|_| {
                at(
                    TransformError::new(
                        TransformErrorKind::Limit,
                        "embedding.index",
                        "index exceeds integer range",
                    ),
                    completed,
                    before,
                )
            })?;
            data.push(item);
        }
        usage.prompt_tokens = usage
            .prompt_tokens
            .checked_add(part.usage.prompt_tokens)
            .ok_or_else(|| {
                at(
                    TransformError::invalid_result(
                        "usage.prompt_tokens",
                        "aggregate token count overflow",
                    ),
                    completed,
                    before,
                )
            })?;
        usage.total_tokens = usage
            .total_tokens
            .checked_add(part.usage.total_tokens)
            .ok_or_else(|| {
                at(
                    TransformError::invalid_result(
                        "usage.total_tokens",
                        "aggregate token count overflow",
                    ),
                    completed,
                    before,
                )
            })?;
        // Bound accumulated output as well as each independently bounded call.
        codec::encode_json(&data, options.codec)
            .map_err(|error| at(packing::encoding(error), completed, before))?;
    }
    let value = o::CreateEmbeddingResponseBody::builder(
        data,
        context.upstream_model,
        o::EmbeddingListObject::List,
        usage,
    )
    .build();
    codec::encode_json(&value, options.codec)
        .map_err(|error| at(packing::encoding(error), completed, value.data.len()))?;
    Ok(EmbeddingBatchResult {
        output: Converted { value, report },
        completed_calls: completed,
    })
}
